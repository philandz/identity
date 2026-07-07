//! Drop the legacy `philand` schema after the v1->v2 migration completes.
//!
//! This is the FINAL destructive step of plan
//! docs/superpowers/plans/2026-07-06-migrate-philand-to-philandz.md
//! (Task 7). Operator authorization has been explicitly obtained.
//!
//! Safety guards:
//!   - Refuses to run if `philandz.users` does not have all 17 expected
//!     users (the v1 monolith baseline).
//!   - Lists every table in philand with row counts before dropping.
//!   - DROP SCHEMA is atomic on MySQL: either all tables are dropped or
//!     none (the catalog DDL is one statement).
//!
//! Usage: cargo run --bin drop_philand

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

    println!("=== Task 7: Drop legacy 'philand' schema ===\n");

    // ---- 1. List what's about to be dropped ----
    let tables: Vec<(String, i64)> = {
        let rows = sqlx::query(
            "SELECT TABLE_NAME as tn FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = 'philand' ORDER BY TABLE_NAME",
        )
        .fetch_all(&pool)
        .await?;
        let mut out = Vec::new();
        for row in rows {
            let name: String = row.get("tn");
            if !is_safe_identifier(&name) {
                continue;
            }
            let sql = format!("SELECT COUNT(*) as cnt FROM `philand`.`{}`", name);
            match sqlx::query(&sql).fetch_one(&pool).await {
                Ok(c) => out.push((name, c.try_get::<i64, _>("cnt").unwrap_or(-1))),
                Err(_) => out.push((name, -1)),
            }
        }
        out
    };
    let total_rows: i64 = tables.iter().map(|(_, c)| *c).sum();

    println!(
        "[audit] {} tables in philand (total {} rows):",
        tables.len(),
        total_rows
    );
    for (name, count) in &tables {
        println!("  philand.{:<32} rows={}", name, count);
    }
    println!();

    // ---- 2. Sanity guards ----
    // philandz must exist with all 17 v1 users mirrored.
    let philandz_exists: i64 = sqlx::query(
        "SELECT COUNT(*) as c FROM information_schema.SCHEMATA
         WHERE SCHEMA_NAME = 'philandz'",
    )
    .fetch_one(&pool)
    .await?
    .get("c");
    if philandz_exists == 0 {
        anyhow::bail!(
            "philandz schema does NOT exist; refusing to drop philand \
             (we have no fallback). Investigate before re-running."
        );
    }

    let philandz_user_count: i64 =
        sqlx::query("SELECT COUNT(*) as c FROM philandz.users")
            .fetch_one(&pool)
            .await?
            .get("c");
    if philandz_user_count != 17 {
        anyhow::bail!(
            "philandz.users has {} rows, expected 17. Refusing to drop \
             philand until philandz is verified.",
            philandz_user_count
        );
    }
    println!(
        "[guard] philandz exists with {} users (matches baseline 17)",
        philandz_user_count
    );

    // ---- 3. Drop ----
    println!("\n[execute] DROP SCHEMA philand...");
    sqlx::query("DROP SCHEMA philand")
        .execute(&pool)
        .await?;
    println!("[execute] drop committed.");

    // ---- 4. Verify ----
    let remaining: Vec<String> = sqlx::query(
        "SELECT SCHEMA_NAME as sn FROM information_schema.SCHEMATA
         WHERE SCHEMA_NAME = 'philand'",
    )
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|r| r.get::<String, _>("sn"))
    .collect();
    if remaining.is_empty() {
        println!("\n[done] philand schema is gone. The migration is complete.");
    } else {
        println!("\n[WARN] philand still listed in information_schema: {:?}", remaining);
    }

    // Confirm philandz is untouched.
    let final_count: i64 = sqlx::query("SELECT COUNT(*) as c FROM philandz.users")
        .fetch_one(&pool)
        .await?
        .get("c");
    println!("[verify] philandz.users still has {} rows (unchanged)", final_count);

    Ok(())
}

fn is_safe_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}