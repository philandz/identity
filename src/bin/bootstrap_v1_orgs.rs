//! Bootstrap a "Personal" organization for each v1 user that does not
//! already belong to any organization.
//!
//! The v1 `philand` schema had no `organizations` table — that was a v2
//! concept.  When v1 users were migrated to `philandz`, they arrived
//! without any org membership.  This binary fixes that by creating a
//! Personal org for each v1 user and adding them as the owner.
//!
//! Idempotent: re-running is a no-op for users who already have an org.
//!
//! Run with: cargo run --bin bootstrap_v1_orgs

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use sqlx::Row;
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL")?;
    let url = if url.contains('?') {
        format!("{}&multiStatements=true", url)
    } else {
        format!("{}?multiStatements=true", url)
    };
    let pool = sqlx::MySqlPool::connect(&url).await?;

    println!("=== Bootstrap Personal org for v1 users ===\n");

    // ---- Find all users that are NOT yet a member of any org ----
    let v1_user_ids: Vec<(String, String)> = sqlx::query(
        "SELECT u.id, u.email
         FROM philandz.users u
         WHERE NOT EXISTS (
             SELECT 1 FROM philandz.organization_members om
             WHERE BINARY om.user_id = BINARY u.id
         )
         ORDER BY u.email",
    )
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|r| (r.get::<String, _>("id"), r.get::<String, _>("email")))
    .collect();

    println!("[plan] {} users need a Personal org:", v1_user_ids.len());
    for (_id, email) in &v1_user_ids {
        println!("  {}", email);
    }
    println!();

    // ---- For each, create an organization + member row in one transaction ----
    let mut created = 0;
    let mut skipped = 0;
    for (user_id, email) in &v1_user_ids {
        let org_id = uuid::Uuid::new_v4().to_string();
        let org_name = format!("{}'s Personal", email.split('@').next().unwrap_or("user"));

        // Check org doesn't already exist (race guard)
        let exists: i64 =
            sqlx::query("SELECT COUNT(*) as c FROM philandz.organizations WHERE id = ?")
                .bind(&org_id)
                .fetch_one(&pool)
                .await?
                .get("c");
        if exists > 0 {
            skipped += 1;
            continue;
        }

        let mut tx = pool.begin().await?;
        sqlx::query(
            "INSERT INTO philandz.organizations (id, name, owner_user_id, status) VALUES (?, ?, ?, 'active')",
        )
        .bind(&org_id)
        .bind(&org_name)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO philandz.organization_members (org_id, user_id, org_role, status) VALUES (?, ?, 'owner', 'active')",
        )
        .bind(&org_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        println!("  + created org for {} ({}) as owner", email, &org_id[..8]);
        created += 1;
    }

    println!(
        "\n[done] created {} org(s), skipped {} (already existed)",
        created, skipped
    );

    // ---- Verify ----
    let orphans: i64 = sqlx::query(
        "SELECT COUNT(*) as c FROM philandz.users u
         WHERE NOT EXISTS (
             SELECT 1 FROM philandz.organization_members om
             WHERE BINARY om.user_id = BINARY u.id
         )",
    )
    .fetch_one(&pool)
    .await?
    .get("c");
    println!("[verify] v1 users without any org now: {}", orphans);

    Ok(())
}
