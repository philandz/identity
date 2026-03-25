CREATE TABLE IF NOT EXISTS organization_invitations (
    id            VARCHAR(36) PRIMARY KEY,
    org_id        VARCHAR(36) NOT NULL,
    inviter_id    VARCHAR(36) NOT NULL,
    invitee_email VARCHAR(255) NOT NULL,
    org_role      VARCHAR(20) NOT NULL COMMENT 'admin | member',
    token_hash    VARCHAR(64) NOT NULL UNIQUE,
    status        VARCHAR(20) NOT NULL COMMENT 'pending | accepted | expired | revoked',
    expires_at    TIMESTAMP NOT NULL,
    created_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    FOREIGN KEY (org_id) REFERENCES organizations(id) ON DELETE CASCADE,
    FOREIGN KEY (inviter_id) REFERENCES users(id) ON DELETE CASCADE,
    UNIQUE KEY uk_org_email (org_id, invitee_email)
);
