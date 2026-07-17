//! Migrate `philandz` tables from v1-monolith schema to v2-service schema.
//!
//! The v1 monolith and the v2 service have **incompatible schemas** for
//! most domain tables:
//!   - v1 `budgets` has `owner_id`; v2 expects `org_id`
//!   - v1 `budgets` has `currency_code`; v2 expects `currency`
//!   - v1 `budgets` has no `status`, `deleted_at`, `created_by`,
//!     `updated_by` columns
//!   - v1 `entries` has fewer columns than v2 (no `notes`, `tags`,
//!     `is_recurring`, `has_attachment`, `recurrence_rule`,
//!     `next_occurrence`, `split_group_id`, `split_total`,
//!     `comment_count`, `attachment_count`, `updated_by`,
//!     `deleted_at`)
//!   - v1 `budget_members` is keyed by composite (budget_id, user_id);
//!     v2 has `id` as PK
//!
//! Rather than rewrite the v2 service code, this binary adds the missing
//! v2 columns to the existing v1 tables and backfills them from the v1
//! columns (e.g., `org_id = owner_id`, `currency = currency_code`,
//! `deleted_at = NULL` where `archived = 0`).
//!
//! Idempotent — re-running is a no-op. Uses information_schema checks to
//! detect existing columns.
//!
//! Run with: cargo run --bin migrate_v1_to_v2
//!
//! After running, restart budget/entry services so they pick up the new
//! schema.

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

    println!("=== Migrate philandz from v1-monolith to v2-service schema ===\n");

    // Helper: does column exist in a table?
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

    // ----- budgets: v1 (owner_id, currency_code, archived, ...) → v2 (org_id, currency, status, deleted_at, created_by, updated_by) -----
    if !col_exists("budgets", "org_id").await? {
        sqlx::query("ALTER TABLE philandz.budgets ADD COLUMN org_id VARCHAR(36) NULL AFTER id")
            .execute(&pool)
            .await?;
        sqlx::query("UPDATE philandz.budgets SET org_id = owner_id")
            .execute(&pool)
            .await?;
        sqlx::query("ALTER TABLE philandz.budgets MODIFY COLUMN org_id VARCHAR(36) NOT NULL")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE INDEX idx_budgets_org_id ON philandz.budgets (org_id)")
            .execute(&pool)
            .await?;
        println!("[alter] budgets.org_id added (backfilled from owner_id)");
    }
    if !col_exists("budgets", "currency").await? {
        sqlx::query(
            "ALTER TABLE philandz.budgets ADD COLUMN currency VARCHAR(8) NULL AFTER budget_type",
        )
        .execute(&pool)
        .await?;
        sqlx::query("UPDATE philandz.budgets SET currency = currency_code")
            .execute(&pool)
            .await?;
        sqlx::query(
            "ALTER TABLE philandz.budgets MODIFY COLUMN currency VARCHAR(8) NOT NULL DEFAULT 'VND'",
        )
        .execute(&pool)
        .await?;
        println!("[alter] budgets.currency added (backfilled from currency_code)");
    }
    if !col_exists("budgets", "status").await? {
        sqlx::query("ALTER TABLE philandz.budgets ADD COLUMN status VARCHAR(16) NOT NULL DEFAULT 'active' AFTER currency")
            .execute(&pool).await?;
        sqlx::query("UPDATE philandz.budgets SET status = CASE WHEN archived = 1 THEN 'archived' ELSE 'active' END")
            .execute(&pool).await?;
        sqlx::query("CREATE INDEX idx_budgets_status ON philandz.budgets (status)")
            .execute(&pool)
            .await?;
        println!("[alter] budgets.status added (backfilled from archived)");
    }
    if !col_exists("budgets", "created_by").await? {
        sqlx::query(
            "ALTER TABLE philandz.budgets ADD COLUMN created_by VARCHAR(36) NULL AFTER status",
        )
        .execute(&pool)
        .await?;
        sqlx::query("UPDATE philandz.budgets SET created_by = owner_id")
            .execute(&pool)
            .await?;
        println!("[alter] budgets.created_by added (backfilled from owner_id)");
    }
    if !col_exists("budgets", "updated_by").await? {
        sqlx::query(
            "ALTER TABLE philandz.budgets ADD COLUMN updated_by VARCHAR(36) NULL AFTER created_by",
        )
        .execute(&pool)
        .await?;
        println!("[alter] budgets.updated_by added");
    }
    if !col_exists("budgets", "deleted_at").await? {
        sqlx::query(
            "ALTER TABLE philandz.budgets ADD COLUMN deleted_at BIGINT NULL AFTER updated_by",
        )
        .execute(&pool)
        .await?;
        println!("[alter] budgets.deleted_at added");
    }

    // ----- budget_members: composite (budget_id, user_id) → add id (UNIQUE) -----
    // Aiven requires a primary key, so we keep the composite PK and
    // add id as a UNIQUE NOT NULL column. The v2 service's INSERT
    // statement explicitly provides id, so this works.
    if !col_exists("budget_members", "id").await? {
        // Add id as VARCHAR(36) NULL first (must populate before making NOT NULL)
        sqlx::query(
            "ALTER TABLE philandz.budget_members ADD COLUMN id VARCHAR(36) NULL AFTER user_id",
        )
        .execute(&pool)
        .await?;
        // Populate id with a deterministic UUID derived from the
        // (budget_id, user_id) pair so it's stable across re-runs.
        sqlx::query(
            "UPDATE philandz.budget_members
             SET id = CONCAT(
                 LPAD(HEX(SUBSTRING(CONCAT(MD5(CONCAT(budget_id, '|', user_id)), 1, 4)), 8, '0'), '-',
                 LPAD(HEX(SUBSTRING(MD5(CONCAT(budget_id, '|', user_id)), 5, 2)), 4, '0'), '-',
                 LPAD(HEX(SUBSTRING(MD5(CONCAT(budget_id, '|', user_id)), 7, 2)), 4, '0'), '-',
                 LPAD(HEX(SUBSTRING(MD5(CONCAT(budget_id, '|', user_id)), 9, 2)), 4, '0'), '-',
                 LPAD(HEX(SUBSTRING(MD5(CONCAT(budget_id, '|', user_id)), 11, 6)), 12, '0')
             )",
        )
        .execute(&pool).await?;
        // Now make id NOT NULL UNIQUE
        sqlx::query("ALTER TABLE philandz.budget_members MODIFY COLUMN id VARCHAR(36) NOT NULL")
            .execute(&pool)
            .await?;
        sqlx::query("ALTER TABLE philandz.budget_members ADD UNIQUE KEY uq_budget_members_id (id)")
            .execute(&pool)
            .await?;
        println!("[alter] budget_members.id added (UNIQUE NOT NULL — composite PK kept for Aiven)");
    }
    if !col_exists("budget_members", "created_at").await? {
        sqlx::query(
            "ALTER TABLE philandz.budget_members ADD COLUMN created_at BIGINT NULL AFTER role",
        )
        .execute(&pool)
        .await?;
        sqlx::query("UPDATE philandz.budget_members SET created_at = UNIX_TIMESTAMP()")
            .execute(&pool)
            .await?;
        println!("[alter] budget_members.created_at added");
    }
    if !col_exists("budget_members", "updated_at").await? {
        sqlx::query("ALTER TABLE philandz.budget_members ADD COLUMN updated_at BIGINT NULL AFTER created_at")
            .execute(&pool).await?;
        sqlx::query("UPDATE philandz.budget_members SET updated_at = UNIX_TIMESTAMP()")
            .execute(&pool)
            .await?;
        println!("[alter] budget_members.updated_at added");
    }
    // Convert role enum → VARCHAR(16) (v2 service expects varchar)
    let role_type: String = sqlx::query(
        "SELECT DATA_TYPE as dt FROM information_schema.columns
         WHERE table_schema = 'philandz' AND table_name = 'budget_members' AND column_name = 'role'",
    )
    .fetch_one(&pool)
    .await?
    .get("dt");
    if role_type.to_lowercase() == "enum"
        || role_type.to_lowercase() == "enum('owner','manager','contributor','viewer')"
    {
        sqlx::query("ALTER TABLE philandz.budget_members MODIFY COLUMN role VARCHAR(16) NOT NULL DEFAULT 'viewer'")
            .execute(&pool).await?;
        println!("[alter] budget_members.role converted from enum → VARCHAR(16)");
    }

    // ----- categories: v1 has budget_id + name + kind + is_hidden + color + icon + timestamps -----
    if !col_exists("categories", "deleted_at").await? {
        sqlx::query(
            "ALTER TABLE philandz.categories ADD COLUMN deleted_at BIGINT NULL AFTER updated_at",
        )
        .execute(&pool)
        .await?;
        println!("[alter] categories.deleted_at added");
    }
    if !col_exists("categories", "created_by").await? {
        sqlx::query("ALTER TABLE philandz.categories ADD COLUMN created_by VARCHAR(36) NULL AFTER updated_at")
            .execute(&pool).await?;
        println!("[alter] categories.created_by added");
    }
    if !col_exists("categories", "updated_by").await? {
        sqlx::query("ALTER TABLE philandz.categories ADD COLUMN updated_by VARCHAR(36) NULL AFTER created_by")
            .execute(&pool).await?;
        println!("[alter] categories.updated_by added");
    }
    // Convert kind enum → VARCHAR(20)
    let kind_type: String = sqlx::query(
        "SELECT DATA_TYPE as dt FROM information_schema.columns
         WHERE table_schema = 'philandz' AND table_name = 'categories' AND column_name = 'kind'",
    )
    .fetch_one(&pool)
    .await?
    .get("dt");
    if kind_type.to_lowercase().starts_with("enum") {
        sqlx::query("ALTER TABLE philandz.categories MODIFY COLUMN kind VARCHAR(20) NOT NULL")
            .execute(&pool)
            .await?;
        println!("[alter] categories.kind converted from enum → VARCHAR(20)");
    }

    // Convert categories.created_at / updated_at from DATETIME → BIGINT.
    // Category service expects i64 (Unix timestamp) per its FromRow derive.
    // The v1 schema had these as DATETIME; the v2 service can't decode them.
    // Safe round-trip: UNIX_TIMESTAMP('2025-10-22 07:30:03') = 1761103803.
    for col in &["created_at", "updated_at"] {
        let dt: String = sqlx::query(
            "SELECT DATA_TYPE as dt FROM information_schema.columns
             WHERE table_schema = 'philandz' AND table_name = 'categories' AND column_name = ?",
        )
        .bind(col)
        .fetch_one(&pool)
        .await?
        .get("dt");
        if dt.to_lowercase() == "datetime" {
            // Add a temporary BIGINT column, backfill from existing DATETIME,
            // drop the old column, rename the new one.  This preserves data.
            let tmp_col = format!("__new_{col}");
            sqlx::query(&format!(
                "ALTER TABLE philandz.categories ADD COLUMN {} BIGINT NULL",
                tmp_col
            ))
            .execute(&pool)
            .await?;
            sqlx::query(&format!(
                "UPDATE philandz.categories SET {} = UNIX_TIMESTAMP({})",
                tmp_col, col
            ))
            .execute(&pool)
            .await?;
            sqlx::query(&format!(
                "ALTER TABLE philandz.categories DROP COLUMN {}",
                col
            ))
            .execute(&pool)
            .await?;
            sqlx::query(&format!(
                "ALTER TABLE philandz.categories CHANGE COLUMN {} {} BIGINT NOT NULL DEFAULT 0",
                tmp_col, col
            ))
            .execute(&pool)
            .await?;
            println!(
                "[convert] categories.{} DATETIME → BIGINT (UNIX_TIMESTAMP applied)",
                col
            );
        }
    }

    // Convert budgets.created_at / updated_at from DATETIME → BIGINT.
    // Budget service expects i64 (Unix timestamp).
    for col in &["created_at", "updated_at"] {
        let dt: String = sqlx::query(
            "SELECT DATA_TYPE as dt FROM information_schema.columns
             WHERE table_schema = 'philandz' AND table_name = 'budgets' AND column_name = ?",
        )
        .bind(col)
        .fetch_one(&pool)
        .await?
        .get("dt");
        if dt.to_lowercase() == "datetime" {
            let tmp_col = format!("__new_{col}");
            sqlx::query(&format!(
                "ALTER TABLE philandz.budgets ADD COLUMN {} BIGINT NULL",
                tmp_col
            ))
            .execute(&pool)
            .await?;
            sqlx::query(&format!(
                "UPDATE philandz.budgets SET {} = UNIX_TIMESTAMP({})",
                tmp_col, col
            ))
            .execute(&pool)
            .await?;
            sqlx::query(&format!("ALTER TABLE philandz.budgets DROP COLUMN {}", col))
                .execute(&pool)
                .await?;
            sqlx::query(&format!(
                "ALTER TABLE philandz.budgets CHANGE COLUMN {} {} BIGINT NOT NULL DEFAULT 0",
                tmp_col, col
            ))
            .execute(&pool)
            .await?;
            println!(
                "[convert] budgets.{} DATETIME → BIGINT (UNIX_TIMESTAMP applied)",
                col
            );
        }
    }

    // ----- entries: add v2-specific columns -----
    for (col, sql_type, default) in &[
        ("notes", "TEXT", "DEFAULT NULL"),
        ("tags", "VARCHAR(500)", "DEFAULT NULL"),
        ("is_recurring", "TINYINT(1)", "NOT NULL DEFAULT 0"),
        ("has_attachment", "TINYINT(1)", "NOT NULL DEFAULT 0"),
        ("recurrence_rule", "VARCHAR(255)", "DEFAULT NULL"),
        ("next_occurrence", "DATE", "DEFAULT NULL"),
        ("split_group_id", "CHAR(36)", "DEFAULT NULL"),
        ("split_total", "BIGINT", "DEFAULT NULL"),
        ("comment_count", "INT", "NOT NULL DEFAULT 0"),
        ("attachment_count", "INT", "NOT NULL DEFAULT 0"),
        ("deleted_at", "BIGINT", "DEFAULT NULL"),
    ] {
        if !col_exists("entries", col).await? {
            let sql = format!(
                "ALTER TABLE philandz.entries ADD COLUMN {} {} {}",
                col, sql_type, default
            );
            sqlx::query(&sql).execute(&pool).await?;
            println!("[alter] entries.{} added ({})", col, sql_type);
        }
    }
    // Convert entries.kind enum → VARCHAR(20)
    let entry_kind_type: String = sqlx::query(
        "SELECT DATA_TYPE as dt FROM information_schema.columns
         WHERE table_schema = 'philandz' AND table_name = 'entries' AND column_name = 'kind'",
    )
    .fetch_one(&pool)
    .await?
    .get("dt");
    if entry_kind_type.to_lowercase().starts_with("enum") {
        sqlx::query("ALTER TABLE philandz.entries MODIFY COLUMN kind VARCHAR(20) NOT NULL")
            .execute(&pool)
            .await?;
        println!("[alter] entries.kind converted from enum → VARCHAR(20)");
    }
    // entries.currency_code: v1 is varchar(10), v2 expects CHAR(3)
    let currency_ml: Option<i64> = {
        use sqlx::Row;
        sqlx::query(
            "SELECT CHARACTER_MAXIMUM_LENGTH as ml
             FROM information_schema.columns
             WHERE table_schema = 'philandz' AND table_name = 'entries' AND column_name = 'currency_code'",
        )
        .fetch_optional(&pool)
        .await?
        .map(|r| r.try_get::<i64, _>("ml").ok())
        .flatten()
    };
    if currency_ml != Some(3) {
        sqlx::query("ALTER TABLE philandz.entries MODIFY COLUMN currency_code CHAR(3) NOT NULL DEFAULT 'VND'")
            .execute(&pool).await?;
        println!("[alter] entries.currency_code narrowed to CHAR(3)");
    }

    // ----- entry_attachments: missing file_id, file_name -----
    if !col_exists("entry_attachments", "file_id").await? {
        sqlx::query("ALTER TABLE philandz.entry_attachments ADD COLUMN file_id CHAR(36) NULL AFTER entry_id")
            .execute(&pool).await?;
        println!("[alter] entry_attachments.file_id added");
    }
    if !col_exists("entry_attachments", "file_name").await? {
        sqlx::query("ALTER TABLE philandz.entry_attachments ADD COLUMN file_name VARCHAR(512) NOT NULL DEFAULT '' AFTER file_id")
            .execute(&pool).await?;
        println!("[alter] entry_attachments.file_name added");
    }
    if !col_exists("entry_attachments", "deleted_at").await? {
        sqlx::query("ALTER TABLE philandz.entry_attachments ADD COLUMN deleted_at BIGINT NULL")
            .execute(&pool)
            .await?;
        println!("[alter] entry_attachments.deleted_at added");
    }

    // ----- entry_comments: add deleted_at -----
    if !col_exists("entry_comments", "deleted_at").await? {
        sqlx::query("ALTER TABLE philandz.entry_comments ADD COLUMN deleted_at BIGINT NULL")
            .execute(&pool)
            .await?;
        println!("[alter] entry_comments.deleted_at added");
    }

    println!("\n[done] v1 → v2 schema migration complete");
    println!("[next] Restart budget/entry services so they pick up the new schema");

    Ok(())
}
