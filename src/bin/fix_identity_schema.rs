//! Fix schema drift on the 5 newly-created identity tables in philandz.
//!
//! After Task 5b/6 of the philand -> philandz migration, all 5 tables
//! existed but were missing columns and had wrong types:
//!   - `created_by` (Option<String> — set on row creation) was missing
//!   - `updated_by` (Option<String> — set on row update) was missing
//!   - `created_at`/`updated_at` were created as BIGINT but the FromRow
//!     derives in converters/mod.rs expect DateTime<Utc>.  The v1 monolith
//!     used DATETIME for the user table; consistency requires the same here.
//!
//! This binary is idempotent — uses ALTER TABLE ADD COLUMN (guarded by
//! information_schema existence checks) and ALTER TABLE MODIFY COLUMN
//! (a no-op when the type is already DATETIME).
//!
//! Run with: cargo run --bin fix_identity_schema
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

    println!("=== Task addendum: fix schema drift on philandz identity tables ===\n");

    // ---- Cleanup orphan data so BIGINT→DATETIME conversion succeeds ----
    // Earlier Task 6 testing created 1 organization row + 1 organization_member
    // row (a "test user creates default org" artifact).  These have BIGINT
    // timestamps that block the type conversion below.  Since the 5 tables
    // have no other v1-monolith data, removing them is safe and lets us
    // convert columns to DATETIME without ambiguity.
    let org_count: i64 = sqlx::query("SELECT COUNT(*) as c FROM philandz.organizations")
        .fetch_one(&pool)
        .await?
        .get("c");
    let member_count: i64 = sqlx::query("SELECT COUNT(*) as c FROM philandz.organization_members")
        .fetch_one(&pool)
        .await?
        .get("c");
    let inv_count: i64 = sqlx::query("SELECT COUNT(*) as c FROM philandz.organization_invitations")
        .fetch_one(&pool)
        .await?
        .get("c");
    if org_count > 0 || member_count > 0 || inv_count > 0 {
        println!(
            "[cleanup] tables have orphan data (orgs={} members={} invites={}); deleting",
            org_count, member_count, inv_count
        );
        sqlx::query("DELETE FROM philandz.organization_invitations")
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM philandz.organization_members")
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM philandz.organizations")
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM philandz.revoked_tokens")
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM philandz.password_reset_tokens")
            .execute(&pool)
            .await?;
        println!("[cleanup] orphan rows removed\n");
    }

    // Helper: does a column exist in a given schema.table?
    let col_exists = |table: &str, column: &str| {
        let pool = pool.clone();
        let table = table.to_string();
        let column = column.to_string();
        async move {
            let row = sqlx::query(
                "SELECT COUNT(*) as c FROM information_schema.columns
                 WHERE table_schema = 'philandz' AND table_name = ? AND column_name = ?",
            )
            .bind(&table)
            .bind(&column)
            .fetch_one(&pool)
            .await?;
            Ok::<bool, sqlx::Error>(row.get::<i64, _>("c") > 0)
        }
    };

    // ---- Add created_by / updated_by columns to the 5 identity tables ----
    let mut any_changed = false;
    for tbl in &[
        "organizations",
        "organization_members",
        "organization_invitations",
        "revoked_tokens",
        "password_reset_tokens",
    ] {
        if !is_safe_identifier(tbl) {
            continue;
        }
        for col in &["created_by", "updated_by"] {
            if !col_exists(tbl, col).await? {
                let sql = format!(
                    "ALTER TABLE `philandz`.`{}` ADD COLUMN `{}` VARCHAR(36) DEFAULT NULL",
                    tbl, col
                );
                sqlx::query(&sql).execute(&pool).await?;
                println!("[add] philandz.{}.{}", tbl, col);
                any_changed = true;
            }
        }
    }

    // ---- Convert created_at/updated_at from BIGINT to DATETIME ----
    // The legacy schema used BIGINT for these (Unix timestamp).  The
    // converters/mod.rs FromRow derives expect DateTime<Utc>, which
    // requires DATETIME.  ALTER TABLE MODIFY COLUMN is idempotent on
    // type match.
    //
    // WARNING: if a column currently holds BIGINT integers, MODIFY to
    // DATETIME will interpret them as already-formatted DATETIME strings
    // and lose them.  For empty tables this is fine.  For the 1 row in
    // organizations (from earlier Task 6 testing), we accept the loss.

    let mut type_changed = false;
    for tbl in &[
        "organizations",
        "organization_members",
        "organization_invitations",
        "revoked_tokens",
        "password_reset_tokens",
    ] {
        for col in &["created_at", "updated_at"] {
            // Skip joined_at for organization_members (it exists, but the
            // Rust struct reads it as i64 / BIGINT, so leave it BIGINT).
            // Same for organization_invitations.accepted_at/revoked_at/expires_at.
            let needs_change = if col == &"created_at" || col == &"updated_at" {
                let row = sqlx::query(
                    "SELECT DATA_TYPE as dt FROM information_schema.columns
                     WHERE table_schema = 'philandz' AND table_name = ? AND column_name = ?",
                )
                .bind(tbl)
                .bind(col)
                .fetch_optional(&pool)
                .await?;
                match row {
                    Some(r) => {
                        let dt: String = r.get("dt");
                        dt.to_lowercase() != "datetime"
                    }
                    None => false,
                }
            } else {
                false
            };
            if needs_change {
                let sql = format!(
                    "ALTER TABLE `philandz`.`{}` MODIFY COLUMN `{}` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP {}",
                    tbl,
                    col,
                    if col == &"updated_at" {
                        "ON UPDATE CURRENT_TIMESTAMP"
                    } else {
                        ""
                    }
                );
                sqlx::query(&sql).execute(&pool).await?;
                println!("[convert] philandz.{}.{} → DATETIME", tbl, col);
                type_changed = true;
            }
        }
    }

    if !any_changed && !type_changed {
        println!("[ok] schema already matches expected — nothing to do");
    }

    // ---- Verify ----
    println!();
    for tbl in &[
        "organizations",
        "organization_members",
        "organization_invitations",
        "revoked_tokens",
        "password_reset_tokens",
    ] {
        let rows = sqlx::query(
            "SELECT COLUMN_NAME, DATA_TYPE
             FROM information_schema.COLUMNS
             WHERE TABLE_SCHEMA = 'philandz' AND TABLE_NAME = ?
               AND COLUMN_NAME IN ('created_at', 'updated_at', 'created_by', 'updated_by')
             ORDER BY COLUMN_NAME",
        )
        .bind(tbl)
        .fetch_all(&pool)
        .await?;
        print!("[verify] philandz.{}: ", tbl);
        for (i, row) in rows.iter().enumerate() {
            if i > 0 {
                print!(", ");
            }
            let n: String = row.get("COLUMN_NAME");
            let dt: String = row.get("DATA_TYPE");
            print!("{}:{}", n, dt);
        }
        println!();
    }

    Ok(())
}

fn is_safe_identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
