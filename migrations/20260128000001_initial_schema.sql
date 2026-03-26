-- Users table
CREATE TABLE IF NOT EXISTS users (
    id VARCHAR(36) PRIMARY KEY, -- UUID
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    user_type VARCHAR(20) NOT NULL COMMENT 'normal | super_admin',
    status VARCHAR(20) NOT NULL COMMENT 'active | disabled',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,

-- Base fields support (optional, but good for consistency)
deleted_at TIMESTAMP NULL,
    created_by VARCHAR(36),
    updated_by VARCHAR(36)
);

-- Organizations table
CREATE TABLE IF NOT EXISTS organizations (
    id VARCHAR(36) PRIMARY KEY, -- UUID
    name VARCHAR(255) NOT NULL,
    owner_user_id VARCHAR(36) NOT NULL,
    status VARCHAR(20) NOT NULL COMMENT 'active | disabled',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP NULL,
    created_by VARCHAR(36),
    updated_by VARCHAR(36),
    FOREIGN KEY (owner_user_id) REFERENCES users (id)
);

-- Organization Members table
CREATE TABLE IF NOT EXISTS organization_members (
    org_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    org_role VARCHAR(20) NOT NULL COMMENT 'owner | admin | member',
    status VARCHAR(20) NOT NULL COMMENT 'active | invited',
    joined_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (org_id, user_id),
    FOREIGN KEY (org_id) REFERENCES organizations (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);