-- Authoritative schema for the 5 identity tables in philandz.
-- Loaded by fix_identity_schema.rs which DROPs and RECREATEs them.
-- After this runs, columns match what converters/mod.rs FromRow derives expect.

CREATE TABLE `philandz`.`organizations` (
    `id`            VARCHAR(36)  NOT NULL PRIMARY KEY,
    `name`          VARCHAR(255) NOT NULL,
    `owner_user_id` VARCHAR(36)  NOT NULL,
    `status`        VARCHAR(20)  NOT NULL DEFAULT 'active',
    `created_at`    DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at`    DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    `deleted_at`    DATETIME     DEFAULT NULL,
    `created_by`    VARCHAR(36)  DEFAULT NULL,
    `updated_by`    VARCHAR(36)  DEFAULT NULL,
    INDEX `idx_organizations_owner` (`owner_user_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE `philandz`.`organization_members` (
    `org_id`        VARCHAR(36)  NOT NULL,
    `user_id`       VARCHAR(36)  NOT NULL,
    `org_role`      VARCHAR(20)  NOT NULL,
    `status`        VARCHAR(20)  NOT NULL DEFAULT 'active',
    `joined_at`     DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at`    DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    `created_by`    VARCHAR(36)  DEFAULT NULL,
    `updated_by`    VARCHAR(36)  DEFAULT NULL,
    PRIMARY KEY (`org_id`, `user_id`),
    INDEX `idx_org_members_user` (`user_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE `philandz`.`organization_invitations` (
    `id`               VARCHAR(36)  NOT NULL PRIMARY KEY,
    `org_id`           VARCHAR(36)  NOT NULL,
    `email`            VARCHAR(255) NOT NULL,
    `org_role`         VARCHAR(20)  NOT NULL,
    `inviter_user_id`  VARCHAR(36)  NOT NULL,
    `token_hash`       VARCHAR(64)  NOT NULL,
    `expires_at`       BIGINT       NOT NULL,
    `accepted_at`      BIGINT       DEFAULT NULL,
    `revoked_at`       BIGINT       DEFAULT NULL,
    `created_at`       DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at`       DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    `created_by`       VARCHAR(36)  DEFAULT NULL,
    `updated_by`       VARCHAR(36)  DEFAULT NULL,
    INDEX `idx_invitations_org`   (`org_id`),
    INDEX `idx_invitations_email` (`email`),
    INDEX `idx_invitations_token` (`token_hash`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE `philandz`.`revoked_tokens` (
    `token_hash`  VARCHAR(64)  NOT NULL PRIMARY KEY,
    `user_id`     VARCHAR(36)  NOT NULL,
    `expires_at`  BIGINT       NOT NULL,
    `created_at`  DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at`  DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    `created_by`  VARCHAR(36)  DEFAULT NULL,
    `updated_by`  VARCHAR(36)  DEFAULT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE `philandz`.`password_reset_tokens` (
    `id`           VARCHAR(36)  NOT NULL PRIMARY KEY,
    `user_id`      VARCHAR(36)  NOT NULL,
    `token_hash`   VARCHAR(64)  NOT NULL,
    `expires_at`   BIGINT       NOT NULL,
    `used_at`      BIGINT       DEFAULT NULL,
    `created_at`   DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated_at`   DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    `created_by`   VARCHAR(36)  DEFAULT NULL,
    `updated_by`   VARCHAR(36)  DEFAULT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
