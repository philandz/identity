-- One-time codes emailed to logged-in users when they request a password
-- change. Distinct from `password_reset_tokens` (which is used by the
-- forgot-password flow for unauthenticated callers). Same shape, different
-- lifecycle: short TTL, brute-force protection via attempts/max_attempts.

CREATE TABLE IF NOT EXISTS password_change_otps (
    id           VARCHAR(36)    NOT NULL PRIMARY KEY,
    user_id      VARCHAR(36)    NOT NULL,
    otp_hash     VARCHAR(64)    NOT NULL,
    expires_at   TIMESTAMP      NOT NULL,
    attempts     TINYINT UNSIGNED NOT NULL DEFAULT 0,
    max_attempts TINYINT UNSIGNED NOT NULL DEFAULT 5,
    used_at      TIMESTAMP      NULL,
    created_at   TIMESTAMP      NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_password_change_otps_user FOREIGN KEY (user_id)
        REFERENCES users(id) ON DELETE CASCADE,
    INDEX idx_password_change_otps_user_active (user_id, used_at, expires_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;