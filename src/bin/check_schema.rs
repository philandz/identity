//! One-shot diagnostic: print columns of philandz.users so we can debug
//! "Field 'created_at' doesn't have a default value" registration failures.
//!
//! Read-only by design — does NOT mutate state.  If you need to fix a
//! schema issue, use `migrate_schemas.rs` (which is idempotent).
//!
//! Run with: cargo run --bin check_schema
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use sqlx::Row;
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL")?;
    let pool = sqlx::MySqlPool::connect(&url).await?;

    println!("=== philandz.users columns ===");
    let rows = sqlx::query(
        "SELECT COLUMN_NAME, COLUMN_TYPE, COLUMN_DEFAULT, IS_NULLABLE, EXTRA
         FROM information_schema.COLUMNS
         WHERE TABLE_SCHEMA = 'philandz' AND TABLE_NAME = 'users'
         ORDER BY ORDINAL_POSITION",
    )
    .fetch_all(&pool)
    .await?;
    for row in &rows {
        let n: String = row.get("COLUMN_NAME");
        let t: String = row.get("COLUMN_TYPE");
        let d: Option<String> = row.try_get("COLUMN_DEFAULT").ok();
        let nn: String = row.get("IS_NULLABLE");
        let e: String = row.try_get("EXTRA").unwrap_or_default();
        println!(
            "  {:>22} {:<28} nullable={} default={:?} {}",
            n, t, nn, d, e
        );
    }

    println!("\n=== philandz.organizations columns ===");
    let rows = sqlx::query(
        "SELECT COLUMN_NAME, COLUMN_TYPE, COLUMN_DEFAULT, IS_NULLABLE, EXTRA
         FROM information_schema.COLUMNS
         WHERE TABLE_SCHEMA = 'philandz' AND TABLE_NAME = 'organizations'
         ORDER BY ORDINAL_POSITION",
    )
    .fetch_all(&pool)
    .await?;
    for row in &rows {
        let n: String = row.get("COLUMN_NAME");
        let t: String = row.get("COLUMN_TYPE");
        let d: Option<String> = row.try_get("COLUMN_DEFAULT").ok();
        let nn: String = row.get("IS_NULLABLE");
        let e: String = row.try_get("EXTRA").unwrap_or_default();
        println!(
            "  {:>22} {:<28} nullable={} default={:?} {}",
            n, t, nn, d, e
        );
    }

    Ok(())
}