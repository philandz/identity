-- Revoked tokens (JWT blacklist for logout)
CREATE TABLE IF NOT EXISTS revoked_tokens (
    token_hash VARCHAR(64) COLLATE utf8mb4_unicode_ci PRIMARY KEY,   -- SHA-256 hex of the JWT
    user_id    VARCHAR(36) COLLATE utf8mb4_unicode_ci NOT NULL,
    expires_at TIMESTAMP NOT NULL,         -- mirrors JWT exp; safe to prune after
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Password reset tokens
CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id         VARCHAR(36) COLLATE utf8mb4_unicode_ci PRIMARY KEY,
    user_id    VARCHAR(36) COLLATE utf8mb4_unicode_ci NOT NULL,
    token_hash VARCHAR(64) COLLATE utf8mb4_unicode_ci NOT NULL UNIQUE,  -- SHA-256 hex of the random token
    expires_at TIMESTAMP NOT NULL,
    used_at    TIMESTAMP NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
