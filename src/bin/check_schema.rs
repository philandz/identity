//! One-shot diagnostic: print columns of all 5 newly-created identity
//! tables in philandz.  Used to debug schema drift between migrate_schemas.rs
//! and the FromRow derives in converters/mod.rs.
//!
//! Read-only by design — does NOT mutate state.
//!
//! Run with: cargo run --bin check_schema
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use sqlx::Row;
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL")?;
    let pool = sqlx::MySqlPool::connect(&url).await?;

    for tbl in &[
        "users",
        "organizations",
        "organization_members",
        "organization_invitations",
        "revoked_tokens",
        "password_reset_tokens",
    ] {
        println!("=== philandz.{tbl} columns ===");
        let rows = sqlx::query(
            "SELECT COLUMN_NAME, COLUMN_TYPE, COLUMN_DEFAULT, IS_NULLABLE, EXTRA
             FROM information_schema.COLUMNS
             WHERE TABLE_SCHEMA = 'philandz' AND TABLE_NAME = ?
             ORDER BY ORDINAL_POSITION",
        )
        .bind(tbl)
        .fetch_all(&pool)
        .await?;
        if rows.is_empty() {
            println!("  TABLE DOES NOT EXIST\n");
            continue;
        }
        for row in &rows {
            let n: String = row.get("COLUMN_NAME");
            let t: String = row.get("COLUMN_TYPE");
            let d: Option<String> = row.try_get("COLUMN_DEFAULT").ok();
            let nn: String = row.get("IS_NULLABLE");
            let e: String = row.try_get("EXTRA").unwrap_or_default();
            println!("  {:>22} {:<28} nullable={} default={:?} {}", n, t, nn, d, e);
        }
        println!();
    }

    // Show canonical-user account details
    let users = sqlx::query(
        "SELECT email, user_type, status, deleted_at, google_id
         FROM philandz.users
         WHERE email IN ('laphi1612@gmail.com','centaurging99@gmail.com',
                         'admin@philand.local','alice@philand.local','bob@philand.local')
         ORDER BY email",
    )
    .fetch_all(&pool)
    .await?;
    println!("=== User account details (canonical users) ===");
    for row in &users {
        let email: String = row.get("email");
        let ut: String = row.try_get("user_type").unwrap_or_default();
        let status: String = row.try_get("status").unwrap_or_default();
        let da: Option<chrono::DateTime<chrono::Utc>> = row.try_get("deleted_at").ok();
        let gid: Option<String> = row.try_get("google_id").ok();
        println!(
            "  {:<32} user_type={:<14} status={:<10} deleted_at={:?} google_id={:?}",
            email, ut, status, da, gid
        );
    }

    // Show organizations + their owner + members
    println!();
    let orgs = sqlx::query(
        "SELECT o.id, o.name, o.owner_user_id
         FROM philandz.organizations o
         ORDER BY o.name",
    )
    .fetch_all(&pool)
    .await?;
    println!("=== Organizations ===");
    for row in &orgs {
        let id: String = row.get("id");
        let name: String = row.get("name");
        let owner_id: String = row.get("owner_user_id");
        println!("  org={} (id={}) owner_id={}", name, &id[..8], &owner_id[..8]);
    }

    // Print member org_id + user_id pairs; look up emails separately to
    // avoid the cross-table collation mismatch (utf8mb4_unicode_ci vs
    // utf8mb4_0900_ai_ci on different tables).
    let members = sqlx::query(
        "SELECT org_id, user_id, org_role FROM philandz.organization_members ORDER BY org_id",
    )
    .fetch_all(&pool)
    .await?;
    println!("\n=== Organization members ===");
    for row in &members {
        let org_id: String = row.get("org_id");
        let user_id: String = row.get("user_id");
        let role: String = row.try_get("org_role").unwrap_or_default();
        println!("  org={} user={} role={}", &org_id[..8], &user_id[..8], role);
    }

    // Cross-check: which v1 users are members of any org?
    let member_user_ids: Vec<String> = members
        .iter()
        .map(|r| r.get::<String, _>("user_id"))
        .collect();
    let v1_user_ids: Vec<String> = sqlx::query(
        "SELECT id FROM philandz.users WHERE email IN
         ('centaurging99@gmail.com','laphi1612@gmail.com','admin@philand.local',
          'alice@philand.local','bob@philand.local','test@example.com')",
    )
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|r| r.get::<String, _>("id"))
    .collect();
    println!("\n=== v1 user × org membership ===");
    for v1_id in &v1_user_ids {
        let member_of = member_user_ids.iter().any(|mid| mid == v1_id);
        println!(
            "  user_id={} member_of_any_org={}",
            &v1_id[..8],
            if member_of { "YES" } else { "NO" }
        );
    }

    Ok(())
}