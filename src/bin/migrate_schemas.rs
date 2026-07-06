//! One-time schema migration: `philand` → `philandz`.
//!
//! This binary creates the `philandz` schema, copies every table from `philand`
//! into it (structure + data), and then creates the 5 identity-domain tables
//! (`organizations`, `organization_members`, `organization_invitations`,
//! `revoked_tokens`, `password_reset_tokens`) that exist in `philandz` but are
//! absent from the legacy `philand` schema.
//!
//! Idempotent: every table-level operation is guarded by `IF NOT EXISTS` or
//! `CREATE TABLE IF NOT EXISTS ... LIKE`, so re-running is safe.
//!
//! Run with: cargo run --bin migrate_schemas

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Aiven MySQL requires ?multiStatements=true for multi-statement SQL
    // (PREPARE / EXECUTE sequences).  Append it to whatever is in DATABASE_URL.
    let database_url = std::env::var("DATABASE_URL")?;
    let database_url = if database_url.contains('?') {
        format!("{}&multiStatements=true", database_url)
    } else {
        format!("{}?multiStatements=true", database_url)
    };

    let pool = sqlx::MySqlPool::connect(&database_url).await?;

    // Wrap everything in a transaction so partial failure rolls back cleanly.
    let mut tx = pool.begin().await?;

    // -------------------------------------------------------------------------
    // Phase 1: Create target schema
    // -------------------------------------------------------------------------
    sqlx::raw_sql("CREATE SCHEMA IF NOT EXISTS philandz")
        .execute(&mut *tx)
        .await?;

    println!("[phase 1] schema 'philandz' created / verified");

    // -------------------------------------------------------------------------
    // Phase 2: Copy every table from `philand` into `philandz`
    //
    // The pattern for each table is:
    //   SET @x := (SELECT COUNT(*) FROM information_schema.TABLES
    //              WHERE TABLE_SCHEMA = 'philand' AND TABLE_NAME = '<tbl>');
    //   SET @sql := IF(@x = 1,
    //                  'CREATE TABLE IF NOT EXISTS philandz.<tbl> LIKE philand.<tbl>',
    //                  'SELECT 1');
    //   PREPARE s FROM @sql; EXECUTE s; DEALLOCATE PREPARE s;
    //   INSERT IGNORE INTO philandz.<tbl> SELECT * FROM philand.<tbl>;
    //
    // `INSERT IGNORE` makes the copy idempotent: if rows already exist the
    // insert is a no-op (duplicate key is silently skipped).
    // -------------------------------------------------------------------------

    let tables = [
        "users",
        "budgets",
        "budget_members",
        "budget_transfers",
        "categories",
        "comment_mentions",
        "entries",
        "entry_attachments",
        "entry_comments",
        "notifications",
        "password_change_otps",
        "password_resets",
        "platform_settings",
        "platform_settings_public",
        "user_oauth_providers",
    ];

    for tbl in &tables {
        let migration_sql = format!(
            "SET @x := (SELECT COUNT(*) FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = 'philand' AND TABLE_NAME = '{tbl}'); \
             SET @sql := IF(@x = 1, \
             'CREATE TABLE IF NOT EXISTS philandz.{tbl} LIKE philand.{tbl}', \
             'SELECT 1'); \
             PREPARE s FROM @sql; EXECUTE s; DEALLOCATE PREPARE s; \
             INSERT IGNORE INTO philandz.{tbl} SELECT * FROM philand.{tbl}",
        );

        sqlx::raw_sql(&migration_sql).execute(&mut *tx).await?;
        println!("[phase 2] migrated table: philand.{tbl} → philandz.{tbl}");
    }

    println!(
        "[phase 2] {} legacy tables copied (or verified) from philand → philandz",
        tables.len()
    );

    // -------------------------------------------------------------------------
    // Phase 3: Create the 5 identity-domain tables that exist in `philandz`
    // but are absent from the legacy `philand` schema.  These tables are
    // defined by identity's domain model and are needed for the identity
    // service to function against `philandz`.
    //
    // Idempotency: tables are created with `IF NOT EXISTS` so re-running is
    // safe UNLESS the table was created with a previous (buggy) schema.
    // The CREATE statement below is the authoritative one.  If you need to
    // apply a schema change to an existing table, drop it first (or migrate
    // in place with ALTER) — `IF NOT EXISTS` will skip the create.
    // -------------------------------------------------------------------------

    // `organizations` — top-level tenant container
    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS philandz.organizations (
            id              VARCHAR(36)  NOT NULL PRIMARY KEY,
            name            VARCHAR(255) NOT NULL,
            owner_user_id   VARCHAR(36)  NOT NULL,
            status          VARCHAR(20)  NOT NULL DEFAULT 'active',
            created_at      BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
            updated_at      BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
            deleted_at      BIGINT       DEFAULT NULL,
            INDEX idx_organizations_owner (owner_user_id)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4
        "#,
    )
    .execute(&mut *tx)
    .await?;
    println!("[phase 3] created table: philandz.organizations");

    // `organization_members` — many-to-many user↔org join with role
    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS philandz.organization_members (
            org_id      VARCHAR(36)  NOT NULL,
            user_id     VARCHAR(36)  NOT NULL,
            org_role    VARCHAR(20)  NOT NULL,
            status      VARCHAR(20)  NOT NULL DEFAULT 'active',
            joined_at   BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
            updated_at  BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
            PRIMARY KEY (org_id, user_id),
            INDEX idx_org_members_user (user_id)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4
        "#,
    )
    .execute(&mut *tx)
    .await?;
    println!("[phase 3] created table: philandz.organization_members");

    // `organization_invitations` — pending invites to join an org
    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS philandz.organization_invitations (
            id              VARCHAR(36)  NOT NULL PRIMARY KEY,
            org_id          VARCHAR(36)  NOT NULL,
            email           VARCHAR(255) NOT NULL,
            org_role        VARCHAR(20)  NOT NULL,
            inviter_user_id VARCHAR(36)  NOT NULL,
            token_hash      VARCHAR(64)  NOT NULL,
            expires_at      BIGINT       NOT NULL,
            accepted_at     BIGINT       DEFAULT NULL,
            revoked_at      BIGINT       DEFAULT NULL,
            created_at      BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
            INDEX idx_invitations_org   (org_id),
            INDEX idx_invitations_email (email),
            INDEX idx_invitations_token (token_hash)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4
        "#,
    )
    .execute(&mut *tx)
    .await?;
    println!("[phase 3] created table: philandz.organization_invitations");

    // `revoked_tokens` — JWT blocklist for logout / rotation
    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS philandz.revoked_tokens (
            token_hash  VARCHAR(64)  NOT NULL PRIMARY KEY,
            user_id     VARCHAR(36)  NOT NULL,
            expires_at  BIGINT       NOT NULL,
            created_at  BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP())
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4
        "#,
    )
    .execute(&mut *tx)
    .await?;
    println!("[phase 3] created table: philandz.revoked_tokens");

    // `password_reset_tokens` — time-limited OTP-based reset tokens
    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS philandz.password_reset_tokens (
            id          VARCHAR(36)  NOT NULL PRIMARY KEY,
            user_id     VARCHAR(36)  NOT NULL,
            token_hash  VARCHAR(64)  NOT NULL,
            expires_at  BIGINT       NOT NULL,
            used_at     BIGINT       DEFAULT NULL,
            created_at  BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP())
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4
        "#,
    )
    .execute(&mut *tx)
    .await?;
    println!("[phase 3] created table: philandz.password_reset_tokens");

    // -------------------------------------------------------------------------
    // Commit the transaction — all phases must succeed or none are applied.
    // -------------------------------------------------------------------------
    tx.commit().await?;
    println!("\n[done] migration committed successfully.");
    println!("       'philandz' schema is ready with all legacy tables and identity tables.");
    Ok(())
}
