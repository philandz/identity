-- Add Google OAuth columns to the legacy `philand.users` table.
--
-- The legacy `philand` schema is owned by the v1 monolith but the v2 identity
-- service queries it (via `philand.users`, etc.) for unified user management.
-- Migrations use `philand.<table>` explicitly because the connection's default
-- database (`defaultdb`) is a sqlx bookkeeping DB, not the real schema.
--
-- Idempotent — each ALTER is gated by an information_schema check wrapped in
-- a stored procedure so the migration can re-run after partial / dirty state.

-- ---------------------------------------------------------------------------
-- Helper procedure (dropped at the end to keep DB tidy).
-- ---------------------------------------------------------------------------
DROP PROCEDURE IF EXISTS mig_add_col_if_missing;
DROP PROCEDURE IF EXISTS mig_add_idx_if_missing;

CREATE PROCEDURE mig_add_col_if_missing(
    IN p_table  VARCHAR(64),
    IN p_column VARCHAR(64),
    IN p_ddl    TEXT
)
BEGIN
    DECLARE v_exist INT DEFAULT 0;
    SELECT COUNT(*) INTO v_exist
    FROM information_schema.columns
    WHERE table_schema = 'philand'
      AND table_name   = p_table
      AND column_name  = p_column;
    IF v_exist = 0 THEN
        SET @ddl = p_ddl;
        PREPARE stmt FROM @ddl;
        EXECUTE stmt;
        DEALLOCATE PREPARE stmt;
    END IF;
END;

CREATE PROCEDURE mig_add_idx_if_missing(
    IN p_table VARCHAR(64),
    IN p_index VARCHAR(64),
    IN p_ddl   TEXT
)
BEGIN
    DECLARE v_exist INT DEFAULT 0;
    SELECT COUNT(*) INTO v_exist
    FROM information_schema.statistics
    WHERE table_schema = 'philand'
      AND table_name   = p_table
      AND index_name   = p_index;
    IF v_exist = 0 THEN
        SET @ddl = p_ddl;
        PREPARE stmt FROM @ddl;
        EXECUTE stmt;
        DEALLOCATE PREPARE stmt;
    END IF;
END;

-- ---------------------------------------------------------------------------
-- Add missing columns to philand.users (idempotent).
-- ---------------------------------------------------------------------------
-- Add display_name, user_type, status, deleted_at, created_by, updated_by,
-- google_id, google_email, google_avatar. No AFTER clauses — order doesn't
-- matter functionally, and adding AFTER status when status doesn't exist yet
-- would fail. Subsequent ALTERs (e.g. `AFTER google_id`) reference columns
-- that already exist by that point.
CALL mig_add_col_if_missing('users', 'display_name',  'ALTER TABLE philand.users ADD COLUMN display_name VARCHAR(255) NULL');
CALL mig_add_col_if_missing('users', 'user_type',     'ALTER TABLE philand.users ADD COLUMN user_type VARCHAR(20) NOT NULL DEFAULT ''normal''');
CALL mig_add_col_if_missing('users', 'status',        'ALTER TABLE philand.users ADD COLUMN status VARCHAR(20) NOT NULL DEFAULT ''active''');
CALL mig_add_col_if_missing('users', 'deleted_at',    'ALTER TABLE philand.users ADD COLUMN deleted_at BIGINT NULL');
CALL mig_add_col_if_missing('users', 'created_by',    'ALTER TABLE philand.users ADD COLUMN created_by VARCHAR(36) NULL');
CALL mig_add_col_if_missing('users', 'updated_by',    'ALTER TABLE philand.users ADD COLUMN updated_by VARCHAR(36) NULL');
CALL mig_add_col_if_missing('users', 'google_id',     'ALTER TABLE philand.users ADD COLUMN google_id VARCHAR(128) NULL');
CALL mig_add_col_if_missing('users', 'google_email',  'ALTER TABLE philand.users ADD COLUMN google_email VARCHAR(255) NULL');
CALL mig_add_col_if_missing('users', 'google_avatar', 'ALTER TABLE philand.users ADD COLUMN google_avatar VARCHAR(512) NULL');

CALL mig_add_idx_if_missing('users', 'uk_users_google_id', 'ALTER TABLE philand.users ADD UNIQUE INDEX uk_users_google_id (google_id)');

-- ---------------------------------------------------------------------------
-- Support tables in `philand` schema (idempotent — IF NOT EXISTS).
-- ---------------------------------------------------------------------------
-- Note: FOREIGN KEY constraints intentionally omitted — philand.users.id is
-- char(36) (legacy monolith schema) and FKs can be problematic across mixed
-- column types. Application code enforces referential integrity. Indexes are
-- still added for query performance.

CREATE TABLE IF NOT EXISTS philand.user_oauth_providers (
    id           VARCHAR(36)  NOT NULL PRIMARY KEY,
    user_id      VARCHAR(36)  NOT NULL,
    provider     VARCHAR(20)  NOT NULL COMMENT 'google',
    provider_id  VARCHAR(128) NOT NULL,
    email        VARCHAR(255) NOT NULL,
    created_at   BIGINT       NOT NULL,
    UNIQUE KEY uk_provider (provider, provider_id),
    INDEX idx_user_oauth_user (user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS philand.platform_settings (
    `key`            VARCHAR(64)  NOT NULL PRIMARY KEY,
    value_ciphertext TEXT         NOT NULL,
    updated_by       VARCHAR(36)  NULL,
    created_at       TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at       TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS philand.platform_settings_public (
    `key`       VARCHAR(64)  NOT NULL PRIMARY KEY,
    value_json  JSON         NOT NULL,
    updated_by  VARCHAR(36)  NULL,
    updated_at  TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS philand.password_change_otps (
    id           VARCHAR(36)    NOT NULL PRIMARY KEY,
    user_id      VARCHAR(36)    NOT NULL,
    otp_hash     VARCHAR(64)    NOT NULL,
    expires_at   TIMESTAMP      NOT NULL,
    attempts     TINYINT UNSIGNED NOT NULL DEFAULT 0,
    max_attempts TINYINT UNSIGNED NOT NULL DEFAULT 5,
    used_at      TIMESTAMP      NULL,
    created_at   TIMESTAMP      NOT NULL DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_password_change_otps_user_active (user_id, used_at, expires_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- ---------------------------------------------------------------------------
-- Cleanup helper procedures.
-- ---------------------------------------------------------------------------
DROP PROCEDURE IF EXISTS mig_add_col_if_missing;
DROP PROCEDURE IF EXISTS mig_add_idx_if_missing;