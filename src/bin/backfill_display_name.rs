//! Backfill `display_name = name` for users where `display_name` is NULL
//! or empty.  Necessary after the philand→philandz migration because legacy
//! v1-monolith rows have `display_name IS NULL` (the v1 schema only had
//! `name`).  Without this backfill, sqlx FromRow derives that expect
//! `display_name: String` panic with "unexpected null" when decoding
//! legacy rows.
//!
//! Idempotent: re-running is a no-op.
//!
//! Run with: cargo run --bin backfill_display_name

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use sqlx::Row;
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL")?;
    let pool = sqlx::MySqlPool::connect(&url).await?;

    println!("=== Backfill display_name from name ===\n");

    // ---- 1. Preview: who would be affected ----
    let affected = sqlx::query(
        "SELECT email, name, display_name
         FROM philandz.users
         WHERE display_name IS NULL OR display_name = ''
         ORDER BY email",
    )
    .fetch_all(&pool)
    .await?;
    println!("[preview] {} rows would be backfilled:", affected.len());
    for row in &affected {
        let email: String = row.get("email");
        let name: Option<String> = row.try_get("name").ok();
        let dn: Option<String> = row.try_get("display_name").ok();
        println!(
            "  {}: name={:?} display_name={:?} → display_name = {:?}",
            email, name, dn, name
        );
    }
    println!();

    // ---- 2. Run the UPDATE ----
    let result = sqlx::query(
        "UPDATE philandz.users
         SET display_name = name
         WHERE display_name IS NULL OR display_name = ''",
    )
    .execute(&pool)
    .await?;
    println!(
        "[execute] updated {} row(s)",
        result.rows_affected()
    );

    // ---- 3. Verify ----
    let remaining_null = sqlx::query(
        "SELECT COUNT(*) as c FROM philandz.users
         WHERE display_name IS NULL OR display_name = ''",
    )
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("c");
    println!("[verify] rows with NULL/empty display_name now: {}", remaining_null);

    Ok(())
}