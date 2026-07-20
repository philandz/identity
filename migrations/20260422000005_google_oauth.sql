-- Google OAuth support
CREATE TABLE IF NOT EXISTS user_oauth_providers (
    id           VARCHAR(36)  COLLATE utf8mb4_unicode_ci NOT NULL PRIMARY KEY,
    user_id      VARCHAR(36)  COLLATE utf8mb4_unicode_ci NOT NULL,
    provider     VARCHAR(20)  COLLATE utf8mb4_unicode_ci NOT NULL COMMENT 'google',
    provider_id  VARCHAR(128) COLLATE utf8mb4_unicode_ci NOT NULL,
    email        VARCHAR(255) COLLATE utf8mb4_unicode_ci NOT NULL,
    created_at   BIGINT       NOT NULL,
    UNIQUE KEY uk_provider (provider, provider_id),
    INDEX idx_user_oauth_user (user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

ALTER TABLE `users` ADD COLUMN google_id VARCHAR(128) COLLATE utf8mb4_unicode_ci DEFAULT NULL;
ALTER TABLE `users` ADD COLUMN google_email VARCHAR(255) COLLATE utf8mb4_unicode_ci DEFAULT NULL;
ALTER TABLE `users` ADD COLUMN google_avatar VARCHAR(512) COLLATE utf8mb4_unicode_ci DEFAULT NULL;
ALTER TABLE `users` ADD UNIQUE INDEX uk_users_google_id (google_id);
