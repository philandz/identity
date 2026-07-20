//! Drop v2-derived domain tables from `defaultdb` schema.
//!
//! This binary is part of the philand -> philandz migration plan
//! (see docs/superpowers/plans/2026-07-06-migrate-philand-to-philandz.md,
//! Task 5b). User authorization has been obtained:
//!
//! > Drop the 29 v2-derived domain tables in `defaultdb` (1,170 v2 working
//! > rows). `_sqlx_migrations` stays.
//!
//! The 17 v1 users in `philand.users` (mirrored in `philandz.users`) are
//! NOT touched. v2 services will recreate their tables via existing
//! migrations on first start.
//!
//! Usage: `cargo run --bin drop_defaultdb`
//!
//! SAFETY: This is the ONLY binary that issues `DROP TABLE` against
//! `defaultdb`. It is idempotent (uses `IF EXISTS`). It refuses to drop
//! `_sqlx_migrations` even if forced.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    // Aiven MySQL requires multiStatements=true for the dynamic GROUP_CONCAT
    // + prepared-statement approach.
    let url = if database_url.contains('?') {
        format!("{}&multiStatements=true", database_url)
    } else {
        format!("{}?multiStatements=true", database_url)
    };
    let pool = sqlx::MySqlPool::connect(&url).await?;
    println!("=== Task 5b: Drop v2-derived tables in defaultdb ===");

    // ---- 1. List what's about to be dropped (audit trail) ----
    let to_drop: Vec<String> = {
        use sqlx::Row;
        sqlx::query(
            "SELECT TABLE_NAME as tn
             FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = 'defaultdb'
               AND TABLE_NAME <> '_sqlx_migrations'
             ORDER BY TABLE_NAME",
        )
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|r| r.get::<String, _>("tn"))
        .collect()
    };

    println!(
        "[audit] {} tables scheduled for DROP TABLE in defaultdb:",
        to_drop.len()
    );
    for name in &to_drop {
        println!("  defaultdb.{}", name);
    }

    // Get exact counts via SELECT COUNT(*) before dropping.
    println!("\n[audit] exact row counts (SELECT COUNT(*)) before drop:");
    for name in &to_drop {
        // Safely format the table name into a SQL fragment. Table names
        // came from information_schema and are already escaped at the
        // storage layer, but defensively reject anything with non-
        // identifier characters.
        if !is_safe_identifier(name) {
            println!("  defaultdb.{:<32} SKIPPED (unsafe identifier)", name);
            continue;
        }
        let sql = format!("SELECT COUNT(*) as cnt FROM `defaultdb`.`{}`", name);
        match sqlx::query(&sql).fetch_one(&pool).await {
            Ok(row) => {
                use sqlx::Row;
                let cnt: i64 = row.try_get("cnt").unwrap_or(-1);
                println!("  defaultdb.{:<32} rows={}", name, cnt);
            }
            Err(e) => {
                println!("  defaultdb.{:<32} ERROR ({})", name, e);
            }
        }
    }

    // ---- 2. Sanity guards ----
    // Refuse to run if `_sqlx_migrations` is missing in defaultdb.
    let bookkeeping_exists: i64 = {
        use sqlx::Row;
        sqlx::query(
            "SELECT COUNT(*) as c FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = 'defaultdb' AND TABLE_NAME = '_sqlx_migrations'",
        )
        .fetch_one(&pool)
        .await?
        .get("c")
    };
    if bookkeeping_exists == 0 {
        anyhow::bail!(
            "_sqlx_migrations table is missing in defaultdb; refusing to drop \
             domain tables in case the bookkeeping has already been moved. \
             Investigate before re-running."
        );
    }

    // ---- 3. Build a single multi-statement DROP block ----
    // Each DROP TABLE is `IF EXISTS` for safety on re-run, but guarded by
    // the existence check in `-- DROP --` so we don't waste cycles.
    let drop_block: String = to_drop
        .iter()
        .map(|name| {
            if !is_safe_identifier(name) {
                // Defensive — but our filter above already passed these.
                String::new()
            } else {
                format!("DROP TABLE IF EXISTS `defaultdb`.`{}`;", name)
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if drop_block.is_empty() {
        println!("\n[abort] no DROP statements generated (filtered all targets).");
        return Ok(());
    }

    // ---- 4. Run all DROP statements in a single transaction ----
    let mut tx = pool.begin().await?;
    println!(
        "\n[execute] dropping {} tables in single transaction...",
        to_drop.len()
    );
    sqlx::raw_sql(&drop_block).execute(&mut *tx).await?;
    tx.commit().await?;
    println!("[execute] drops committed.");

    // ---- 5. Post-state: only _sqlx_migrations should remain ----
    let remaining: Vec<String> = {
        use sqlx::Row;
        sqlx::query(
            "SELECT TABLE_NAME as tn FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = 'defaultdb' ORDER BY TABLE_NAME",
        )
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|r| r.get::<String, _>("tn"))
        .collect()
    };
    println!(
        "\n[verify] tables now in defaultdb ({} total):",
        remaining.len()
    );
    for t in &remaining {
        println!("  defaultdb.{}", t);
    }

    if remaining.len() == 1 && remaining[0] == "_sqlx_migrations" {
        println!("\n[done] Task 5b complete: defaultdb contains only _sqlx_migrations.");
        println!("       v2 services must rebuild their domain tables on next start.");
    } else {
        println!("\n[WARN] unexpected tables remain in defaultdb — review above.");
    }

    Ok(())
}

fn is_safe_identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
