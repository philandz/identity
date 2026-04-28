-- Google OAuth support
CREATE TABLE IF NOT EXISTS user_oauth_providers (
    id           VARCHAR(36)  NOT NULL PRIMARY KEY,
    user_id      VARCHAR(36)  NOT NULL,
    provider     VARCHAR(20)  NOT NULL COMMENT 'google',
    provider_id  VARCHAR(128) NOT NULL,
    email        VARCHAR(255) NOT NULL,
    created_at   BIGINT       NOT NULL,
    UNIQUE KEY uk_provider (provider, provider_id),
    INDEX idx_user_oauth_user (user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

ALTER TABLE `users` ADD COLUMN google_id VARCHAR(128) DEFAULT NULL;
ALTER TABLE `users` ADD COLUMN google_email VARCHAR(255) DEFAULT NULL;
ALTER TABLE `users` ADD COLUMN google_avatar VARCHAR(512) DEFAULT NULL;
ALTER TABLE `users` ADD UNIQUE INDEX uk_users_google_id (google_id);
