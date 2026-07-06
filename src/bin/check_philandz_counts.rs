//! Quick read-only check of philandz row counts.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use sqlx::Row;
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = sqlx::MySqlPool::connect(&database_url).await?;

    let tables = vec![
        "budget_members", "budget_transfers", "budgets", "categories",
        "comment_mentions", "entries", "entry_attachments", "entry_comments",
        "notifications", "password_change_otps", "password_resets",
        "platform_settings", "platform_settings_public", "user_oauth_providers",
        "users", "organizations", "organization_members", "organization_invitations",
        "revoked_tokens", "password_reset_tokens",
    ];

    println!("[ROW_COUNTS:philandz] table_count={}", tables.len());
    for tbl in &tables {
        let sql = format!("SELECT COUNT(*) as cnt FROM `philandz`.`{}`", tbl);
        match sqlx::query(&sql).fetch_one(&pool).await {
            Ok(row) => {
                let cnt: i64 = row.try_get("cnt").unwrap_or(-1);
                println!("  philandz.{}: {}", tbl, cnt);
            }
            Err(e) => {
                println!("  philandz.{}: ERROR ({})", tbl, e);
            }
        }
    }
    Ok(())
}