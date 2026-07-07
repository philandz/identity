/// One-time read-only snapshot of the live Aiven MySQL instance.
///
/// Run with: cargo run --bin snapshot_db
///
/// Output captures:
///   - All schemas on the server
///   - For every table in `philand`: a real `SELECT COUNT(*)` row count
///   - For every table in `defaultdb`: a real `SELECT COUNT(*)` row count
///   - For every other schema: just a list of its tables (no row counts)
///   - Whether `philandz` schema is present
///   - The full user roster from `philand.users` (id, email, name, google_id)
///
/// This binary is SELECT-only. No INSERT, UPDATE, DELETE, ALTER, TRUNCATE, or
/// DDL is issued against any user table or system table.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = sqlx::MySqlPool::connect(&database_url).await?;

    println!("=== READ-ONLY PRE-MIGRATION SNAPSHOT ===");
    println!("captured_at_utc: {}", chrono::Utc::now().to_rfc3339());
    println!();

    // ---- 1. All schemas on the server ----
    let schemas = sqlx::query(
        "SELECT SCHEMA_NAME as sn FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
    )
    .fetch_all(&pool)
    .await?;
    println!("[SCHEMAS] count={}", schemas.len());
    {
        use sqlx::Row;
        for row in &schemas {
            let s: String = row.get("sn");
            println!("schema: {}", s);
        }
    }
    println!();

    // ---- 2. Is `philandz` present? ----
    let philandz_present: i64 = {
        use sqlx::Row;
        sqlx::query(
            "SELECT COUNT(*) as c FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = 'philandz'",
        )
        .fetch_one(&pool)
        .await?
        .get::<i64, _>("c")
    };
    println!("[philandz_present] {}", if philandz_present > 0 { "YES" } else { "NO" });
    println!();

    // ---- 3. For each schema, list its tables ----
    let all_schemas: Vec<String> = {
        use sqlx::Row;
        schemas
            .iter()
            .map(|r| r.get::<String, _>("sn"))
            .collect()
    };
    for schema in &all_schemas {
        let tbls = sqlx::query(
            "SELECT TABLE_NAME as tn, IFNULL(TABLE_COMMENT, '') as tc
             FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = ?
             ORDER BY TABLE_NAME",
        )
        .bind(schema)
        .fetch_all(&pool)
        .await?;
        println!("[TABLES_IN:{}] count={}", schema, tbls.len());
        {
            use sqlx::Row;
            for row in &tbls {
                let t: String = row.get("tn");
                let tc: String = row.try_get("tc").unwrap_or_default();
                if tc.is_empty() {
                    println!("  table: {}", t);
                } else {
                    println!("  table: {}  /* {} */", t, tc);
                }
            }
        }
        println!();
    }

    // ---- 4. Real row counts for every table in `philand` (if it still exists) ----
    let philand_exists: i64 = {
        use sqlx::Row;
        sqlx::query(
            "SELECT COUNT(*) as c FROM information_schema.SCHEMATA
             WHERE SCHEMA_NAME = 'philand'",
        )
        .fetch_one(&pool)
        .await?
        .get("c")
    };
    if philand_exists == 0 {
        println!("[ROW_COUNTS:philand] schema does not exist (already dropped — post-migration state)");
    } else {
        let philand_tables: Vec<String> = {
            use sqlx::Row;
            sqlx::query(
                "SELECT TABLE_NAME as tn FROM information_schema.TABLES
                 WHERE TABLE_SCHEMA = 'philand' ORDER BY TABLE_NAME",
            )
            .fetch_all(&pool)
            .await?
            .into_iter()
            .map(|r| r.get::<String, _>("tn"))
            .collect()
        };
        println!("[ROW_COUNTS:philand] table_count={}", philand_tables.len());
        for tbl in &philand_tables {
            if !is_safe_identifier(tbl) {
                println!("  philand.{} SKIPPED (unsafe identifier)", tbl);
                continue;
            }
            let sql = format!("SELECT COUNT(*) as cnt FROM `philand`.`{}`", tbl);
            match sqlx::query(&sql).fetch_one(&pool).await {
                Ok(row) => {
                    use sqlx::Row;
                    let cnt: i64 = row.try_get("cnt").unwrap_or(-1);
                    println!("  philand.{}: {}", tbl, cnt);
                }
                Err(e) => {
                    println!("  philand.{}: ERROR ({})", tbl, e);
                }
            }
        }
    }
    println!();

    // ---- 5. Real row counts for every table in `defaultdb` ----
    let defaultdb_tables: Vec<String> = {
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
    println!("[ROW_COUNTS:defaultdb] table_count={}", defaultdb_tables.len());
    for tbl in &defaultdb_tables {
        if !is_safe_identifier(tbl) {
            println!("  defaultdb.{} SKIPPED (unsafe identifier)", tbl);
            continue;
        }
        let sql = format!("SELECT COUNT(*) as cnt FROM `defaultdb`.`{}`", tbl);
        match sqlx::query(&sql).fetch_one(&pool).await {
            Ok(row) => {
                use sqlx::Row;
                let cnt: i64 = row.try_get("cnt").unwrap_or(-1);
                println!("  defaultdb.{}: {}", tbl, cnt);
            }
            Err(e) => {
                println!("  defaultdb.{}: ERROR ({})", tbl, e);
            }
        }
    }
    println!();

    // ---- 5b. Real row counts for every table in `philandz` (post-migration) ----
    let philandz_tables: Vec<String> = {
        use sqlx::Row;
        sqlx::query(
            "SELECT TABLE_NAME as tn FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = 'philandz' ORDER BY TABLE_NAME",
        )
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|r| r.get::<String, _>("tn"))
        .collect()
    };
    println!("[ROW_COUNTS:philandz] table_count={}", philandz_tables.len());
    for tbl in &philandz_tables {
        if !is_safe_identifier(tbl) {
            println!("  philandz.{} SKIPPED (unsafe identifier)", tbl);
            continue;
        }
        let sql = format!("SELECT COUNT(*) as cnt FROM `philandz`.`{}`", tbl);
        match sqlx::query(&sql).fetch_one(&pool).await {
            Ok(row) => {
                use sqlx::Row;
                let cnt: i64 = row.try_get("cnt").unwrap_or(-1);
                println!("  philandz.{}: {}", tbl, cnt);
            }
            Err(e) => {
                println!("  philandz.{}: ERROR ({})", tbl, e);
            }
        }
    }
    println!();
    let users = sqlx::query(
        "SELECT id as i, email as e, name as n, display_name as dn, google_id as gid
         FROM philandz.users ORDER BY email",
    )
    .fetch_all(&pool)
    .await?;
    println!("[USERS:philandz.users] row_count={}", users.len());
    {
        use sqlx::Row;
        for row in &users {
            let id: String = row.get("i");
            let email: String = row.get("e");
            let name: String = row.try_get("n").unwrap_or_default();
            let dn: Option<String> = row.try_get("dn").ok();
            let gid: Option<String> = row.try_get("gid").ok();
            println!(
                "  user id={} email={} name={} display_name={:?} google_id={:?}",
                id, email, name, dn, gid
            );
        }
    }
    println!();

    // ---- 7. Support tables in `philandz` (existence check only) ----
    println!("[SUPPORT_TABLES:philandz]");
    for tbl in &[
        "user_oauth_providers",
        "platform_settings",
        "platform_settings_public",
        "password_change_otps",
    ] {
        let exists = sqlx::query(
            "SELECT 1 as x FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = 'philandz' AND TABLE_NAME = ?",
        )
        .bind(tbl)
        .fetch_optional(&pool)
        .await?
        .is_some();
        println!("  philandz.{}: {}", tbl, if exists { "EXISTS" } else { "MISSING" });
    }
    println!();

    println!("=== END OF SNAPSHOT ===");
    Ok(())
}

fn is_safe_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}
