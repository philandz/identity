/// One-time utility to clean up migration state.
/// Run with: cargo run --bin fix_migrations
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = sqlx::MySqlPool::connect(&database_url).await?;

    // Show current state
    let rows =
        sqlx::query("SELECT version, description, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await?;
    println!("Current migrations in DB:");
    for row in &rows {
        use sqlx::Row;
        let v: i64 = row.try_get("version").unwrap_or(0);
        let d: String = row.try_get("description").unwrap_or_default();
        let s: bool = row.try_get("success").unwrap_or(false);
        println!("  {} | {} | success={}", v, d, s);
    }

    // Remove any migration not in our known files list, or that failed
    let known: &[i64] = &[
        20260128000001,
        20260128000002,
        20260128000003,
        20260408000004,
        20260422000005,
    ];
    for row in &rows {
        use sqlx::Row;
        let v: i64 = row.try_get("version").unwrap_or(0);
        let s: bool = row.try_get("success").unwrap_or(false);
        if !known.contains(&v) || !s {
            let r = sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?")
                .bind(v)
                .execute(&pool)
                .await?;
            println!(
                "Deleted migration {} ({} rows affected)",
                v,
                r.rows_affected()
            );
        }
    }

    println!("Done.");
    Ok(())
}
