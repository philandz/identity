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

    Ok(())
}