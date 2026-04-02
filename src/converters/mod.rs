use crate::pb::common::base as common;
use crate::pb::shared::organization;
use crate::pb::shared::user;
use sqlx::types::chrono::{DateTime, Utc};

// ---------------------------------------------------------------------------
// DB model structs (sqlx::FromRow — must use String for VARCHAR columns)
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
pub struct DbUser {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub user_type: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct DbOrganization {
    pub id: String,
    pub name: String,
    pub owner_user_id: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct DbRevokedToken {
    pub token_hash: String,
    pub user_id: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct DbPasswordResetToken {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct DbOrganizationInvitation {
    pub id: String,
    pub org_id: String,
    pub inviter_id: String,
    pub invitee_email: String,
    pub org_role: String,
    pub token_hash: String,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct DbOrgMemberRow {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub org_role: String,
    pub status: String,
    pub joined_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Enum ↔ DB string conversion helpers (single source of truth)
// ---------------------------------------------------------------------------

/// Convert a `BaseStatus` enum to its DB VARCHAR representation.
pub fn base_status_to_db(status: common::BaseStatus) -> &'static str {
    match status {
        common::BaseStatus::BsActive => "active",
        common::BaseStatus::BsInactive => "inactive",
        common::BaseStatus::BsPending => "pending",
        common::BaseStatus::BsDeleted => "deleted",
        common::BaseStatus::BsArchived => "archived",
        common::BaseStatus::BsBlocked => "blocked",
        common::BaseStatus::BsDisabled => "disabled",
        common::BaseStatus::BsInvited => "invited",
        common::BaseStatus::BsUnknown | common::BaseStatus::BsNone => "unknown",
    }
}

/// Convert a DB VARCHAR string back to a `BaseStatus` enum.
pub fn base_status_from_db(s: &str) -> common::BaseStatus {
    match s {
        "active" => common::BaseStatus::BsActive,
        "inactive" => common::BaseStatus::BsInactive,
        "pending" => common::BaseStatus::BsPending,
        "deleted" => common::BaseStatus::BsDeleted,
        "archived" => common::BaseStatus::BsArchived,
        "blocked" => common::BaseStatus::BsBlocked,
        "disabled" => common::BaseStatus::BsDisabled,
        "invited" => common::BaseStatus::BsInvited,
        _ => common::BaseStatus::BsUnknown,
    }
}

/// Convert a `UserType` enum to its DB VARCHAR representation.
pub fn user_type_to_db(user_type: user::UserType) -> &'static str {
    match user_type {
        user::UserType::UtNormal => "normal",
        user::UserType::UtSuperAdmin => "super_admin",
        user::UserType::UtNone => "unknown",
    }
}

/// Convert a DB VARCHAR string back to a `UserType` enum.
pub fn user_type_from_db(s: &str) -> user::UserType {
    match s {
        "normal" => user::UserType::UtNormal,
        "super_admin" => user::UserType::UtSuperAdmin,
        _ => user::UserType::UtNone,
    }
}

/// Convert an `OrgRole` enum to its DB VARCHAR representation.
pub fn org_role_to_db(role: organization::OrgRole) -> &'static str {
    match role {
        organization::OrgRole::OrOwner => "owner",
        organization::OrgRole::OrAdmin => "admin",
        organization::OrgRole::OrMember => "member",
        organization::OrgRole::OrNone => "none",
    }
}

/// Convert a DB VARCHAR string back to an `OrgRole` enum.
pub fn org_role_from_db(s: &str) -> organization::OrgRole {
    match s {
        "owner" => organization::OrgRole::OrOwner,
        "admin" => organization::OrgRole::OrAdmin,
        "member" => organization::OrgRole::OrMember,
        _ => organization::OrgRole::OrNone,
    }
}

/// Convert a `MemberStatus` enum to its DB VARCHAR representation.
pub fn member_status_to_db(status: organization::MemberStatus) -> &'static str {
    match status {
        organization::MemberStatus::MsActive => "active",
        organization::MemberStatus::MsInvited => "invited",
        organization::MemberStatus::MsNone => "unknown",
    }
}

/// Convert a DB VARCHAR string back to a `MemberStatus` enum.
pub fn member_status_from_db(s: &str) -> organization::MemberStatus {
    match s {
        "active" => organization::MemberStatus::MsActive,
        "invited" => organization::MemberStatus::MsInvited,
        _ => organization::MemberStatus::MsNone,
    }
}

/// Convert an `InvitationStatus` enum to its DB VARCHAR representation.
pub fn invitation_status_to_db(status: organization::InvitationStatus) -> &'static str {
    match status {
        organization::InvitationStatus::IsPending => "pending",
        organization::InvitationStatus::IsAccepted => "accepted",
        organization::InvitationStatus::IsExpired => "expired",
        organization::InvitationStatus::IsRevoked => "revoked",
        organization::InvitationStatus::IsNone => "none",
    }
}

/// Convert a DB VARCHAR string back to an `InvitationStatus` enum.
pub fn invitation_status_from_db(s: &str) -> organization::InvitationStatus {
    match s {
        "pending" => organization::InvitationStatus::IsPending,
        "accepted" => organization::InvitationStatus::IsAccepted,
        "expired" => organization::InvitationStatus::IsExpired,
        "revoked" => organization::InvitationStatus::IsRevoked,
        _ => organization::InvitationStatus::IsNone,
    }
}

// ---------------------------------------------------------------------------
// DB model → Proto message conversions
// ---------------------------------------------------------------------------

impl From<DbUser> for user::User {
    fn from(db_user: DbUser) -> Self {
        let status = base_status_from_db(&db_user.status);
        let user_type = user_type_from_db(&db_user.user_type);

        user::User {
            base: Some(common::Base {
                id: db_user.id,
                created_at: db_user.created_at.timestamp(),
                updated_at: db_user.updated_at.timestamp(),
                deleted_at: db_user.deleted_at.map(|t| t.timestamp()).unwrap_or(0),
                created_by: db_user.created_by.unwrap_or_default(),
                updated_by: db_user.updated_by.unwrap_or_default(),
                status: status as i32,
                ..Default::default()
            }),
            email: db_user.email,
            password_hash: db_user.password_hash,
            display_name: db_user.display_name,
            user_type: user_type as i32,
        }
    }
}

impl From<DbOrganization> for organization::Organization {
    fn from(db_org: DbOrganization) -> Self {
        let status = base_status_from_db(&db_org.status);

        organization::Organization {
            base: Some(common::Base {
                id: db_org.id,
                created_at: db_org.created_at.timestamp(),
                updated_at: db_org.updated_at.timestamp(),
                deleted_at: db_org.deleted_at.map(|t| t.timestamp()).unwrap_or(0),
                created_by: db_org.created_by.unwrap_or_default(),
                updated_by: db_org.updated_by.unwrap_or_default(),
                owner_id: db_org.owner_user_id,
                status: status as i32,
            }),
            name: db_org.name,
        }
    }
}
