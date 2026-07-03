-- Platform-wide settings, split into two tables:
--  * `platform_settings`  — secrets, AES-GCM encrypted at rest, never
--    returned through any RPC.
--  * `platform_settings_public` — non-secret config (from-address, reply-to,
--    enabled flag) that the rest of the system reads at startup.

CREATE TABLE IF NOT EXISTS platform_settings (
    `key`            VARCHAR(64)  NOT NULL PRIMARY KEY,
    value_ciphertext TEXT         NOT NULL,
    updated_by       VARCHAR(36)  NULL,
    created_at       TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at       TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    CONSTRAINT fk_platform_settings_updated_by FOREIGN KEY (updated_by)
        REFERENCES users(id) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS platform_settings_public (
    `key`       VARCHAR(64)  NOT NULL PRIMARY KEY,
    value_json  JSON         NOT NULL,
    updated_by  VARCHAR(36)  NULL,
    updated_at  TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    CONSTRAINT fk_platform_settings_public_updated_by FOREIGN KEY (updated_by)
        REFERENCES users(id) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;