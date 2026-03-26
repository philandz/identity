use crate::converters::{
    base_status_to_db, invitation_status_from_db, invitation_status_to_db, member_status_from_db,
    member_status_to_db, org_role_from_db, org_role_to_db, user_type_to_db, DbOrgMemberRow,
    DbOrganization, DbOrganizationInvitation, DbPasswordResetToken, DbUser,
};
use crate::pb::common::base::BaseStatus;
use crate::pb::shared::organization::{InvitationStatus, MemberStatus, OrgRole};
use crate::pb::shared::user::UserType;
use chrono::{DateTime, Utc};
use sqlx::{MySql, Pool, Row};
use std::sync::Arc;

/// Repository layer — the only module that touches the database.
pub struct IdentityRepository {
    pool: Arc<Pool<MySql>>,
}

/// Typed org summary returned from `find_user_org_summaries`.
pub struct OrgSummaryRow {
    pub id: String,
    pub name: String,
    pub role: OrgRole,
}

pub struct OrgMemberRow {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub role: OrgRole,
    pub status: MemberStatus,
    pub joined_at: i64,
}

pub struct OrganizationInvitationRow {
    pub id: String,
    pub org_id: String,
    pub inviter_id: String,
    pub invitee_email: String,
    pub org_role: OrgRole,
    pub status: InvitationStatus,
    pub expires_at: i64,
    pub created_at: i64,
}

pub struct UpsertOrganizationInvitationParams<'a> {
    pub id: &'a str,
    pub org_id: &'a str,
    pub inviter_id: &'a str,
    pub invitee_email: &'a str,
    pub org_role: OrgRole,
    pub token_hash: &'a str,
    pub status: InvitationStatus,
    pub expires_at: DateTime<Utc>,
}

impl IdentityRepository {
    pub fn new(pool: Arc<Pool<MySql>>) -> Self {
        Self { pool }
    }

    pub async fn insert_user(
        &self,
        id: &str,
        email: &str,
        password_hash: &str,
        display_name: &str,
        user_type: UserType,
        status: BaseStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, display_name, user_type, status) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(id).bind(email).bind(password_hash)
        .bind(display_name)
        .bind(user_type_to_db(user_type))
        .bind(base_status_to_db(status))
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_user_with_default_organization(
        &self,
        user_id: &str,
        email: &str,
        password_hash: &str,
        display_name: &str,
        user_type: UserType,
        user_status: BaseStatus,
        org_id: &str,
        org_name: &str,
        org_role: OrgRole,
        membership_status: MemberStatus,
    ) -> Result<DbUser, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO users (id, email, password_hash, display_name, user_type, status) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(email)
        .bind(password_hash)
        .bind(display_name)
        .bind(user_type_to_db(user_type))
        .bind(base_status_to_db(user_status))
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO organizations (id, name, owner_user_id, status) VALUES (?, ?, ?, ?)",
        )
        .bind(org_id)
        .bind(org_name)
        .bind(user_id)
        .bind(base_status_to_db(BaseStatus::BsActive))
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO organization_members (org_id, user_id, org_role, status) VALUES (?, ?, ?, ?)",
        )
        .bind(org_id)
        .bind(user_id)
        .bind(org_role_to_db(org_role))
        .bind(member_status_to_db(membership_status))
        .execute(&mut *tx)
        .await?;

        let db_user: DbUser = sqlx::query_as(
            "SELECT id, email, password_hash, display_name, user_type, status, created_at, updated_at, deleted_at, created_by, updated_by FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(db_user)
    }

    pub fn is_unique_violation(&self, error: &sqlx::Error) -> bool {
        error.as_database_error().is_some_and(|db_error| {
            let code_match = db_error
                .code()
                .is_some_and(|code| code == "1062" || code == "23000");
            let message_match = db_error
                .message()
                .to_ascii_lowercase()
                .contains("duplicate entry");
            code_match || message_match
        })
    }

    pub async fn find_user_by_id(&self, id: &str) -> Result<Option<DbUser>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, email, password_hash, display_name, user_type, status, created_at, updated_at, deleted_at, created_by, updated_by FROM users WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<DbUser>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, email, password_hash, display_name, user_type, status, created_at, updated_at, deleted_at, created_by, updated_by FROM users WHERE email = ?"
        )
        .bind(email)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn insert_organization(
        &self,
        id: &str,
        name: &str,
        owner_user_id: &str,
        status: BaseStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO organizations (id, name, owner_user_id, status) VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(owner_user_id)
        .bind(base_status_to_db(status))
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_organization_member(
        &self,
        org_id: &str,
        user_id: &str,
        org_role: OrgRole,
        status: MemberStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO organization_members (org_id, user_id, org_role, status) VALUES (?, ?, ?, ?)"
        )
        .bind(org_id).bind(user_id)
        .bind(org_role_to_db(org_role))
        .bind(member_status_to_db(status))
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_user_org_summaries(
        &self,
        user_id: &str,
    ) -> Result<Vec<OrgSummaryRow>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT o.id, o.name, m.org_role
            FROM organizations o
            JOIN organization_members m ON o.id = m.org_id
            WHERE m.user_id = ? AND m.status = ?
            "#,
        )
        .bind(user_id)
        .bind(member_status_to_db(MemberStatus::MsActive))
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let role_str: String = r.get("org_role");
                OrgSummaryRow {
                    id: r.get("id"),
                    name: r.get("name"),
                    role: org_role_from_db(&role_str),
                }
            })
            .collect())
    }

    pub async fn find_user_organizations(
        &self,
        user_id: &str,
    ) -> Result<Vec<DbOrganization>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT o.id, o.name, o.owner_user_id, o.status, o.created_at, o.updated_at, o.deleted_at, o.created_by, o.updated_by
            FROM organizations o
            JOIN organization_members m ON o.id = m.org_id
            WHERE m.user_id = ? AND m.status = ?
            "#,
        )
        .bind(user_id)
        .bind(member_status_to_db(MemberStatus::MsActive))
        .fetch_all(&*self.pool)
        .await
    }

    // -----------------------------------------------------------------------
    // P0: Token revocation (logout)
    // -----------------------------------------------------------------------

    /// Insert a revoked token hash into the blacklist.
    pub async fn insert_revoked_token(
        &self,
        token_hash: &str,
        user_id: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT IGNORE INTO revoked_tokens (token_hash, user_id, expires_at) VALUES (?, ?, ?)",
        )
        .bind(token_hash)
        .bind(user_id)
        .bind(expires_at)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Check whether a token hash has been revoked.
    pub async fn is_token_revoked(&self, token_hash: &str) -> Result<bool, sqlx::Error> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM revoked_tokens WHERE token_hash = ?")
                .bind(token_hash)
                .fetch_optional(&*self.pool)
                .await?;
        Ok(row.is_some())
    }

    // -----------------------------------------------------------------------
    // P0: Password reset tokens
    // -----------------------------------------------------------------------

    /// Insert a new password-reset token record.
    pub async fn insert_password_reset_token(
        &self,
        id: &str,
        user_id: &str,
        token_hash: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Find a password-reset token by its hash (unused and not expired).
    pub async fn find_valid_password_reset_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<DbPasswordResetToken>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT id, user_id, token_hash, expires_at, used_at, created_at
            FROM password_reset_tokens
            WHERE token_hash = ? AND used_at IS NULL AND expires_at > NOW()
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&*self.pool)
        .await
    }

    /// Mark a password-reset token as used.
    pub async fn mark_password_reset_token_used(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE password_reset_tokens SET used_at = NOW() WHERE id = ?")
            .bind(id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // P0: Password update
    // -----------------------------------------------------------------------

    /// Update a user's password hash.
    pub async fn update_user_password(
        &self,
        user_id: &str,
        new_password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(new_password_hash)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // P1: Organization IAM
    // -----------------------------------------------------------------------

    pub async fn find_org_member_role(
        &self,
        org_id: &str,
        user_id: &str,
    ) -> Result<Option<OrgRole>, sqlx::Error> {
        let role: Option<(String,)> = sqlx::query_as(
            "SELECT org_role FROM organization_members WHERE org_id = ? AND user_id = ? AND status = ?",
        )
        .bind(org_id)
        .bind(user_id)
        .bind(member_status_to_db(MemberStatus::MsActive))
        .fetch_optional(&*self.pool)
        .await?;

        Ok(role.map(|(role,)| org_role_from_db(&role)))
    }

    pub async fn list_org_members(&self, org_id: &str) -> Result<Vec<OrgMemberRow>, sqlx::Error> {
        let rows: Vec<DbOrgMemberRow> = sqlx::query_as(
            r#"
            SELECT m.user_id, u.email, u.display_name, m.org_role, m.status, m.joined_at
            FROM organization_members m
            JOIN users u ON u.id = m.user_id
            WHERE m.org_id = ?
            ORDER BY m.joined_at ASC
            "#,
        )
        .bind(org_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| OrgMemberRow {
                user_id: r.user_id,
                email: r.email,
                display_name: r.display_name,
                role: org_role_from_db(&r.org_role),
                status: member_status_from_db(&r.status),
                joined_at: r.joined_at.timestamp(),
            })
            .collect())
    }

    pub async fn upsert_organization_invitation(
        &self,
        params: UpsertOrganizationInvitationParams<'_>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO organization_invitations
                (id, org_id, inviter_id, invitee_email, org_role, token_hash, status, expires_at)
            VALUES
                (?, ?, ?, ?, ?, ?, ?, ?)
            ON DUPLICATE KEY UPDATE
                id = VALUES(id),
                inviter_id = VALUES(inviter_id),
                org_role = VALUES(org_role),
                token_hash = VALUES(token_hash),
                status = VALUES(status),
                expires_at = VALUES(expires_at)
            "#,
        )
        .bind(params.id)
        .bind(params.org_id)
        .bind(params.inviter_id)
        .bind(params.invitee_email)
        .bind(org_role_to_db(params.org_role))
        .bind(params.token_hash)
        .bind(invitation_status_to_db(params.status))
        .bind(params.expires_at)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_valid_invitation_by_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<OrganizationInvitationRow>, sqlx::Error> {
        let row: Option<DbOrganizationInvitation> = sqlx::query_as(
            r#"
            SELECT id, org_id, inviter_id, invitee_email, org_role, token_hash, status, expires_at, created_at, updated_at
            FROM organization_invitations
            WHERE token_hash = ? AND status = ? AND expires_at > NOW()
            "#,
        )
        .bind(token_hash)
        .bind(invitation_status_to_db(InvitationStatus::IsPending))
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| OrganizationInvitationRow {
            id: r.id,
            org_id: r.org_id,
            inviter_id: r.inviter_id,
            invitee_email: r.invitee_email,
            org_role: org_role_from_db(&r.org_role),
            status: invitation_status_from_db(&r.status),
            expires_at: r.expires_at.timestamp(),
            created_at: r.created_at.timestamp(),
        }))
    }

    pub async fn find_user_by_email_active_member_of_org(
        &self,
        org_id: &str,
        email: &str,
    ) -> Result<bool, sqlx::Error> {
        let row: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT 1
            FROM organization_members m
            JOIN users u ON u.id = m.user_id
            WHERE m.org_id = ? AND u.email = ? AND m.status = ?
            "#,
        )
        .bind(org_id)
        .bind(email)
        .bind(member_status_to_db(MemberStatus::MsActive))
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.is_some())
    }

    pub async fn accept_invitation_for_user(
        &self,
        invitation_id: &str,
        org_id: &str,
        user_id: &str,
        role: OrgRole,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("UPDATE organization_invitations SET status = ? WHERE id = ?")
            .bind(invitation_status_to_db(InvitationStatus::IsAccepted))
            .bind(invitation_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            r#"
            INSERT INTO organization_members (org_id, user_id, org_role, status)
            VALUES (?, ?, ?, ?)
            ON DUPLICATE KEY UPDATE org_role = VALUES(org_role), status = VALUES(status)
            "#,
        )
        .bind(org_id)
        .bind(user_id)
        .bind(org_role_to_db(role))
        .bind(member_status_to_db(MemberStatus::MsActive))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn update_org_member_role(
        &self,
        org_id: &str,
        user_id: &str,
        new_role: OrgRole,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            "UPDATE organization_members SET org_role = ? WHERE org_id = ? AND user_id = ? AND status = ?",
        )
        .bind(org_role_to_db(new_role))
        .bind(org_id)
        .bind(user_id)
        .bind(member_status_to_db(MemberStatus::MsActive))
        .execute(&*self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn remove_org_member(&self, org_id: &str, user_id: &str) -> Result<u64, sqlx::Error> {
        let res = sqlx::query("DELETE FROM organization_members WHERE org_id = ? AND user_id = ?")
            .bind(org_id)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;
        Ok(res.rows_affected())
    }
}
