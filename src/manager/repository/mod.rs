use crate::converters::{
    base_status_to_db, invitation_status_from_db, invitation_status_to_db, member_status_from_db,
    member_status_to_db, org_role_from_db, org_role_to_db, user_type_to_db, DbOrganization,
    DbPasswordResetToken, DbUser,
};
use crate::pb::common::base::BaseStatus;
use crate::pb::shared::organization::{InvitationStatus, MemberStatus, OrgRole};
use crate::pb::shared::user::UserType;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

pub struct IdentityRepository {
    inner: philand_storage::repo::Repo,
}

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
    pub async fn new(
        config: &philand_configs::IdentityServiceConfig,
    ) -> Result<Self, philand_storage::StorageError> {
        let inner = philand_storage::repo::Repo::new_repo(config).await?;
        let repo = Self { inner };
        repo.ensure_tables().await?;
        Ok(repo)
    }

    pub fn from_pool(pool: std::sync::Arc<sqlx::Pool<sqlx::MySql>>) -> Self {
        Self {
            inner: philand_storage::repo::Repo::from_pool(pool),
        }
    }

    async fn ensure_tables(&self) -> Result<(), philand_storage::StorageError> {
        let users = table_name(philand_table::table::USERS);
        let organizations = table_name(philand_table::table::ORGANIZATIONS);
        let organization_members = table_name(philand_table::table::ORGANIZATION_MEMBERS);
        let revoked_tokens = table_name(philand_table::table::REVOKED_TOKENS);
        let password_reset_tokens = table_name(philand_table::table::PASSWORD_RESET_TOKENS);
        let organization_invitations = table_name(philand_table::table::ORGANIZATION_INVITATIONS);

        let create_users = format!(
            "CREATE TABLE IF NOT EXISTS {users} (
                id VARCHAR(36) PRIMARY KEY,
                email VARCHAR(255) NOT NULL UNIQUE,
                password_hash VARCHAR(255) NOT NULL,
                display_name VARCHAR(255) NOT NULL,
                user_type VARCHAR(20) NOT NULL COMMENT 'normal | super_admin',
                status VARCHAR(20) NOT NULL COMMENT 'active | disabled',
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                deleted_at TIMESTAMP NULL,
                created_by VARCHAR(36),
                updated_by VARCHAR(36),
                UNIQUE KEY uk_users_email (email)
            )"
        );

        let create_orgs = format!(
            "CREATE TABLE IF NOT EXISTS {organizations} (
                id VARCHAR(36) PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                owner_user_id VARCHAR(36) NOT NULL,
                status VARCHAR(20) NOT NULL COMMENT 'active | disabled',
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                deleted_at TIMESTAMP NULL,
                created_by VARCHAR(36),
                updated_by VARCHAR(36),
                FOREIGN KEY (owner_user_id) REFERENCES {users} (id)
            )"
        );

        let create_org_members = format!(
            "CREATE TABLE IF NOT EXISTS {organization_members} (
                org_id VARCHAR(36) NOT NULL,
                user_id VARCHAR(36) NOT NULL,
                org_role VARCHAR(20) NOT NULL COMMENT 'owner | admin | member',
                status VARCHAR(20) NOT NULL COMMENT 'active | invited',
                joined_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (org_id, user_id),
                UNIQUE KEY uk_org_user (org_id, user_id),
                FOREIGN KEY (org_id) REFERENCES {organizations} (id) ON DELETE CASCADE,
                FOREIGN KEY (user_id) REFERENCES {users} (id) ON DELETE CASCADE
            )"
        );

        let create_revoked_tokens = format!(
            "CREATE TABLE IF NOT EXISTS {revoked_tokens} (
                token_hash VARCHAR(64) PRIMARY KEY,
                user_id VARCHAR(36) NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES {users}(id) ON DELETE CASCADE
            )"
        );

        let create_password_reset_tokens = format!(
            "CREATE TABLE IF NOT EXISTS {password_reset_tokens} (
                id VARCHAR(36) PRIMARY KEY,
                user_id VARCHAR(36) NOT NULL,
                token_hash VARCHAR(64) NOT NULL UNIQUE,
                expires_at TIMESTAMP NOT NULL,
                used_at TIMESTAMP NULL,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES {users}(id) ON DELETE CASCADE
            )"
        );

        let create_org_invitations = format!(
            "CREATE TABLE IF NOT EXISTS {organization_invitations} (
                id VARCHAR(36) PRIMARY KEY,
                org_id VARCHAR(36) NOT NULL,
                inviter_id VARCHAR(36) NOT NULL,
                invitee_email VARCHAR(255) NOT NULL,
                org_role VARCHAR(20) NOT NULL COMMENT 'admin | member',
                token_hash VARCHAR(64) NOT NULL UNIQUE,
                status VARCHAR(20) NOT NULL COMMENT 'pending | accepted | expired | revoked',
                expires_at TIMESTAMP NOT NULL,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (org_id) REFERENCES {organizations}(id) ON DELETE CASCADE,
                FOREIGN KEY (inviter_id) REFERENCES {users}(id) ON DELETE CASCADE,
                UNIQUE KEY uk_org_email (org_id, invitee_email),
                UNIQUE KEY uk_invitation_token (token_hash)
            )"
        );

        self.inner.execute(&create_users).await?;
        self.inner.execute(&create_orgs).await?;
        self.inner.execute(&create_org_members).await?;
        self.inner.execute(&create_revoked_tokens).await?;
        self.inner.execute(&create_password_reset_tokens).await?;
        self.inner.execute(&create_org_invitations).await?;

        Ok(())
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
        let mut data = Map::new();
        data.insert("id".to_string(), Value::String(id.to_string()));
        data.insert("email".to_string(), Value::String(email.to_string()));
        data.insert(
            "password_hash".to_string(),
            Value::String(password_hash.to_string()),
        );
        data.insert(
            "display_name".to_string(),
            Value::String(display_name.to_string()),
        );
        data.insert(
            "user_type".to_string(),
            Value::String(user_type_to_db(user_type).to_string()),
        );
        data.insert(
            "status".to_string(),
            Value::String(base_status_to_db(status).to_string()),
        );

        self.inner
            .create(philand_table::table::USERS, &data)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
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
        self.insert_user(
            user_id,
            email,
            password_hash,
            display_name,
            user_type,
            user_status,
        )
        .await?;

        if let Err(err) = self
            .insert_organization(org_id, org_name, user_id, BaseStatus::BsActive)
            .await
        {
            let _ = self.delete_user_by_id(user_id).await;
            return Err(err);
        }

        if let Err(err) = self
            .insert_organization_member(org_id, user_id, org_role, membership_status)
            .await
        {
            let _ = self.delete_organization_by_id(org_id).await;
            let _ = self.delete_user_by_id(user_id).await;
            return Err(err);
        }

        self.find_user_by_id(user_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
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
        let mut filters = Map::new();
        filters.insert("id".to_string(), Value::String(id.to_string()));
        let row = self
            .inner
            .get(philand_table::table::USERS, &filters)
            .await
            .map_err(map_storage_error)?;
        Ok(row.map(map_to_db_user))
    }

    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<DbUser>, sqlx::Error> {
        let mut filters = Map::new();
        filters.insert("email".to_string(), Value::String(email.to_string()));
        let row = self
            .inner
            .get(philand_table::table::USERS, &filters)
            .await
            .map_err(map_storage_error)?;
        Ok(row.map(map_to_db_user))
    }

    pub async fn insert_organization(
        &self,
        id: &str,
        name: &str,
        owner_user_id: &str,
        status: BaseStatus,
    ) -> Result<(), sqlx::Error> {
        let mut data = Map::new();
        data.insert("id".to_string(), Value::String(id.to_string()));
        data.insert("name".to_string(), Value::String(name.to_string()));
        data.insert(
            "owner_user_id".to_string(),
            Value::String(owner_user_id.to_string()),
        );
        data.insert(
            "status".to_string(),
            Value::String(base_status_to_db(status).to_string()),
        );
        self.inner
            .create(philand_table::table::ORGANIZATIONS, &data)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
    }

    pub async fn insert_organization_member(
        &self,
        org_id: &str,
        user_id: &str,
        org_role: OrgRole,
        status: MemberStatus,
    ) -> Result<(), sqlx::Error> {
        let mut data = Map::new();
        data.insert("org_id".to_string(), Value::String(org_id.to_string()));
        data.insert("user_id".to_string(), Value::String(user_id.to_string()));
        data.insert(
            "org_role".to_string(),
            Value::String(org_role_to_db(org_role).to_string()),
        );
        data.insert(
            "status".to_string(),
            Value::String(member_status_to_db(status).to_string()),
        );
        self.inner
            .create(philand_table::table::ORGANIZATION_MEMBERS, &data)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
    }

    pub async fn find_user_org_summaries(
        &self,
        user_id: &str,
    ) -> Result<Vec<OrgSummaryRow>, sqlx::Error> {
        let mut filters = Map::new();
        filters.insert("user_id".to_string(), Value::String(user_id.to_string()));
        filters.insert("status".to_string(), Value::String("active".to_string()));
        let memberships = self
            .inner
            .list(philand_table::table::ORGANIZATION_MEMBERS, &filters)
            .await
            .map_err(map_storage_error)?;

        let mut out = Vec::new();
        for m in memberships {
            let org_id = string_field(&m, "org_id");
            let mut org_filter = Map::new();
            org_filter.insert("id".to_string(), Value::String(org_id.clone()));
            if let Some(org) = self
                .inner
                .get(philand_table::table::ORGANIZATIONS, &org_filter)
                .await
                .map_err(map_storage_error)?
            {
                out.push(OrgSummaryRow {
                    id: org_id,
                    name: string_field(&org, "name"),
                    role: org_role_from_db(&string_field(&m, "org_role")),
                });
            }
        }
        Ok(out)
    }

    pub async fn find_user_organizations(
        &self,
        user_id: &str,
    ) -> Result<Vec<DbOrganization>, sqlx::Error> {
        let mut filters = Map::new();
        filters.insert("user_id".to_string(), Value::String(user_id.to_string()));
        filters.insert("status".to_string(), Value::String("active".to_string()));
        let memberships = self
            .inner
            .list(philand_table::table::ORGANIZATION_MEMBERS, &filters)
            .await
            .map_err(map_storage_error)?;

        let mut out = Vec::new();
        for m in memberships {
            let org_id = string_field(&m, "org_id");
            let mut org_filter = Map::new();
            org_filter.insert("id".to_string(), Value::String(org_id));
            if let Some(org) = self
                .inner
                .get(philand_table::table::ORGANIZATIONS, &org_filter)
                .await
                .map_err(map_storage_error)?
            {
                out.push(map_to_db_organization(org));
            }
        }
        Ok(out)
    }

    pub async fn insert_revoked_token(
        &self,
        token_hash: &str,
        user_id: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mut data = Map::new();
        data.insert(
            "token_hash".to_string(),
            Value::String(token_hash.to_string()),
        );
        data.insert("user_id".to_string(), Value::String(user_id.to_string()));
        data.insert(
            "expires_at".to_string(),
            Value::String(fmt_db_time(expires_at)),
        );
        self.inner
            .create(philand_table::table::REVOKED_TOKENS, &data)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
    }

    pub async fn is_token_revoked(&self, token_hash: &str) -> Result<bool, sqlx::Error> {
        let mut filters = Map::new();
        filters.insert(
            "token_hash".to_string(),
            Value::String(token_hash.to_string()),
        );
        Ok(self
            .inner
            .get(philand_table::table::REVOKED_TOKENS, &filters)
            .await
            .map_err(map_storage_error)?
            .is_some())
    }

    pub async fn insert_password_reset_token(
        &self,
        id: &str,
        user_id: &str,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mut data = Map::new();
        data.insert("id".to_string(), Value::String(id.to_string()));
        data.insert("user_id".to_string(), Value::String(user_id.to_string()));
        data.insert(
            "token_hash".to_string(),
            Value::String(token_hash.to_string()),
        );
        data.insert(
            "expires_at".to_string(),
            Value::String(fmt_db_time(expires_at)),
        );
        self.inner
            .create(philand_table::table::PASSWORD_RESET_TOKENS, &data)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
    }

    pub async fn find_valid_password_reset_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<DbPasswordResetToken>, sqlx::Error> {
        let mut filters = Map::new();
        filters.insert(
            "token_hash".to_string(),
            Value::String(token_hash.to_string()),
        );
        let row = self
            .inner
            .get(philand_table::table::PASSWORD_RESET_TOKENS, &filters)
            .await
            .map_err(map_storage_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let out = map_to_db_password_reset(row);
        if out.used_at.is_some() || out.expires_at <= Utc::now() {
            return Ok(None);
        }
        Ok(Some(out))
    }

    pub async fn mark_password_reset_token_used(&self, id: &str) -> Result<(), sqlx::Error> {
        let mut data = Map::new();
        data.insert(
            "used_at".to_string(),
            Value::String(fmt_db_time(Utc::now())),
        );
        let mut filters = Map::new();
        filters.insert("id".to_string(), Value::String(id.to_string()));
        self.inner
            .update(philand_table::table::PASSWORD_RESET_TOKENS, &data, &filters)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
    }

    pub async fn update_user_password(
        &self,
        user_id: &str,
        new_password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        let mut data = Map::new();
        data.insert(
            "password_hash".to_string(),
            Value::String(new_password_hash.to_string()),
        );
        let mut filters = Map::new();
        filters.insert("id".to_string(), Value::String(user_id.to_string()));
        self.inner
            .update(philand_table::table::USERS, &data, &filters)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
    }

    pub async fn find_org_member_role(
        &self,
        org_id: &str,
        user_id: &str,
    ) -> Result<Option<OrgRole>, sqlx::Error> {
        let mut filters = Map::new();
        filters.insert("org_id".to_string(), Value::String(org_id.to_string()));
        filters.insert("user_id".to_string(), Value::String(user_id.to_string()));
        filters.insert("status".to_string(), Value::String("active".to_string()));
        let row = self
            .inner
            .get(philand_table::table::ORGANIZATION_MEMBERS, &filters)
            .await
            .map_err(map_storage_error)?;
        Ok(row.map(|r| org_role_from_db(&string_field(&r, "org_role"))))
    }

    pub async fn list_org_members(&self, org_id: &str) -> Result<Vec<OrgMemberRow>, sqlx::Error> {
        let mut filters = Map::new();
        filters.insert("org_id".to_string(), Value::String(org_id.to_string()));
        let rows = self
            .inner
            .list(philand_table::table::ORGANIZATION_MEMBERS, &filters)
            .await
            .map_err(map_storage_error)?;

        let mut out = Vec::new();
        for m in rows {
            let user_id = string_field(&m, "user_id");
            let mut uf = Map::new();
            uf.insert("id".to_string(), Value::String(user_id.clone()));
            if let Some(u) = self
                .inner
                .get(philand_table::table::USERS, &uf)
                .await
                .map_err(map_storage_error)?
            {
                out.push(OrgMemberRow {
                    user_id,
                    email: string_field(&u, "email"),
                    display_name: string_field(&u, "display_name"),
                    role: org_role_from_db(&string_field(&m, "org_role")),
                    status: member_status_from_db(&string_field(&m, "status")),
                    joined_at: datetime_field(&m, "joined_at").timestamp(),
                });
            }
        }
        Ok(out)
    }

    pub async fn upsert_organization_invitation(
        &self,
        params: UpsertOrganizationInvitationParams<'_>,
    ) -> Result<(), sqlx::Error> {
        let mut filters = Map::new();
        filters.insert(
            "org_id".to_string(),
            Value::String(params.org_id.to_string()),
        );
        filters.insert(
            "invitee_email".to_string(),
            Value::String(params.invitee_email.to_string()),
        );

        let mut data = Map::new();
        data.insert("id".to_string(), Value::String(params.id.to_string()));
        data.insert(
            "inviter_id".to_string(),
            Value::String(params.inviter_id.to_string()),
        );
        data.insert(
            "org_role".to_string(),
            Value::String(org_role_to_db(params.org_role).to_string()),
        );
        data.insert(
            "token_hash".to_string(),
            Value::String(params.token_hash.to_string()),
        );
        data.insert(
            "status".to_string(),
            Value::String(invitation_status_to_db(params.status).to_string()),
        );
        data.insert(
            "expires_at".to_string(),
            Value::String(fmt_db_time(params.expires_at)),
        );

        if self
            .inner
            .get(philand_table::table::ORGANIZATION_INVITATIONS, &filters)
            .await
            .map_err(map_storage_error)?
            .is_some()
        {
            self.inner
                .update(
                    philand_table::table::ORGANIZATION_INVITATIONS,
                    &data,
                    &filters,
                )
                .await
                .map(|_| ())
                .map_err(map_storage_error)
        } else {
            data.insert(
                "org_id".to_string(),
                Value::String(params.org_id.to_string()),
            );
            data.insert(
                "invitee_email".to_string(),
                Value::String(params.invitee_email.to_string()),
            );
            self.inner
                .create(philand_table::table::ORGANIZATION_INVITATIONS, &data)
                .await
                .map(|_| ())
                .map_err(map_storage_error)
        }
    }

    pub async fn find_valid_invitation_by_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<OrganizationInvitationRow>, sqlx::Error> {
        let mut filters = Map::new();
        filters.insert(
            "token_hash".to_string(),
            Value::String(token_hash.to_string()),
        );
        let row = self
            .inner
            .get(philand_table::table::ORGANIZATION_INVITATIONS, &filters)
            .await
            .map_err(map_storage_error)?;
        let Some(row) = row else {
            return Ok(None);
        };

        let status = invitation_status_from_db(&string_field(&row, "status"));
        let expires_at = datetime_field(&row, "expires_at").timestamp();
        if status != InvitationStatus::IsPending || expires_at <= Utc::now().timestamp() {
            return Ok(None);
        }

        Ok(Some(OrganizationInvitationRow {
            id: string_field(&row, "id"),
            org_id: string_field(&row, "org_id"),
            inviter_id: string_field(&row, "inviter_id"),
            invitee_email: string_field(&row, "invitee_email"),
            org_role: org_role_from_db(&string_field(&row, "org_role")),
            status,
            expires_at,
            created_at: datetime_field(&row, "created_at").timestamp(),
        }))
    }

    pub async fn find_user_by_email_active_member_of_org(
        &self,
        org_id: &str,
        email: &str,
    ) -> Result<bool, sqlx::Error> {
        let Some(user) = self.find_user_by_email(email).await? else {
            return Ok(false);
        };

        let mut filters = Map::new();
        filters.insert("org_id".to_string(), Value::String(org_id.to_string()));
        filters.insert("user_id".to_string(), Value::String(user.id));
        filters.insert("status".to_string(), Value::String("active".to_string()));
        Ok(self
            .inner
            .get(philand_table::table::ORGANIZATION_MEMBERS, &filters)
            .await
            .map_err(map_storage_error)?
            .is_some())
    }

    pub async fn accept_invitation_for_user(
        &self,
        invitation_id: &str,
        org_id: &str,
        user_id: &str,
        role: OrgRole,
    ) -> Result<(), sqlx::Error> {
        let mut inv_data = Map::new();
        inv_data.insert("status".to_string(), Value::String("accepted".to_string()));
        let mut inv_filter = Map::new();
        inv_filter.insert("id".to_string(), Value::String(invitation_id.to_string()));
        self.inner
            .update(
                philand_table::table::ORGANIZATION_INVITATIONS,
                &inv_data,
                &inv_filter,
            )
            .await
            .map_err(map_storage_error)?;

        let mut member_filter = Map::new();
        member_filter.insert("org_id".to_string(), Value::String(org_id.to_string()));
        member_filter.insert("user_id".to_string(), Value::String(user_id.to_string()));
        let mut member_data = Map::new();
        member_data.insert(
            "org_role".to_string(),
            Value::String(org_role_to_db(role).to_string()),
        );
        member_data.insert("status".to_string(), Value::String("active".to_string()));

        if self
            .inner
            .get(philand_table::table::ORGANIZATION_MEMBERS, &member_filter)
            .await
            .map_err(map_storage_error)?
            .is_some()
        {
            self.inner
                .update(
                    philand_table::table::ORGANIZATION_MEMBERS,
                    &member_data,
                    &member_filter,
                )
                .await
                .map_err(map_storage_error)?;
        } else {
            member_data.insert("org_id".to_string(), Value::String(org_id.to_string()));
            member_data.insert("user_id".to_string(), Value::String(user_id.to_string()));
            self.inner
                .create(philand_table::table::ORGANIZATION_MEMBERS, &member_data)
                .await
                .map_err(map_storage_error)?;
        }

        Ok(())
    }

    pub async fn update_org_member_role(
        &self,
        org_id: &str,
        user_id: &str,
        new_role: OrgRole,
    ) -> Result<u64, sqlx::Error> {
        let mut data = Map::new();
        data.insert(
            "org_role".to_string(),
            Value::String(org_role_to_db(new_role).to_string()),
        );
        let mut filters = Map::new();
        filters.insert("org_id".to_string(), Value::String(org_id.to_string()));
        filters.insert("user_id".to_string(), Value::String(user_id.to_string()));
        filters.insert("status".to_string(), Value::String("active".to_string()));
        self.inner
            .update(philand_table::table::ORGANIZATION_MEMBERS, &data, &filters)
            .await
            .map_err(map_storage_error)
    }

    pub async fn remove_org_member(&self, org_id: &str, user_id: &str) -> Result<u64, sqlx::Error> {
        let mut filters = Map::new();
        filters.insert("org_id".to_string(), Value::String(org_id.to_string()));
        filters.insert("user_id".to_string(), Value::String(user_id.to_string()));
        self.inner
            .delete(philand_table::table::ORGANIZATION_MEMBERS, &filters)
            .await
            .map_err(map_storage_error)
    }

    async fn delete_user_by_id(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let mut filters = Map::new();
        filters.insert("id".to_string(), Value::String(user_id.to_string()));
        self.inner
            .delete(philand_table::table::USERS, &filters)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
    }

    async fn delete_organization_by_id(&self, org_id: &str) -> Result<(), sqlx::Error> {
        let mut filters = Map::new();
        filters.insert("id".to_string(), Value::String(org_id.to_string()));
        self.inner
            .delete(philand_table::table::ORGANIZATIONS, &filters)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
    }
}

fn map_storage_error(err: philand_storage::StorageError) -> sqlx::Error {
    match err {
        philand_storage::StorageError::Sqlx(e) => e,
        other => sqlx::Error::Protocol(other.to_string()),
    }
}

fn map_to_db_user(row: Map<String, Value>) -> DbUser {
    DbUser {
        id: string_field(&row, "id"),
        email: string_field(&row, "email"),
        password_hash: string_field(&row, "password_hash"),
        display_name: string_field(&row, "display_name"),
        user_type: string_field(&row, "user_type"),
        status: string_field(&row, "status"),
        created_at: datetime_field(&row, "created_at"),
        updated_at: datetime_field(&row, "updated_at"),
        deleted_at: opt_datetime_field(&row, "deleted_at"),
        created_by: opt_string_field(&row, "created_by"),
        updated_by: opt_string_field(&row, "updated_by"),
    }
}

fn map_to_db_organization(row: Map<String, Value>) -> DbOrganization {
    DbOrganization {
        id: string_field(&row, "id"),
        name: string_field(&row, "name"),
        owner_user_id: string_field(&row, "owner_user_id"),
        status: string_field(&row, "status"),
        created_at: datetime_field(&row, "created_at"),
        updated_at: datetime_field(&row, "updated_at"),
        deleted_at: opt_datetime_field(&row, "deleted_at"),
        created_by: opt_string_field(&row, "created_by"),
        updated_by: opt_string_field(&row, "updated_by"),
    }
}

fn map_to_db_password_reset(row: Map<String, Value>) -> DbPasswordResetToken {
    DbPasswordResetToken {
        id: string_field(&row, "id"),
        user_id: string_field(&row, "user_id"),
        token_hash: string_field(&row, "token_hash"),
        expires_at: datetime_field(&row, "expires_at"),
        used_at: opt_datetime_field(&row, "used_at"),
        created_at: datetime_field(&row, "created_at"),
    }
}

fn string_field(row: &Map<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn opt_string_field(row: &Map<String, Value>, key: &str) -> Option<String> {
    row.get(key)
        .and_then(Value::as_str)
        .map(std::string::ToString::to_string)
}

fn datetime_field(row: &Map<String, Value>, key: &str) -> DateTime<Utc> {
    opt_datetime_field(row, key).unwrap_or_else(Utc::now)
}

fn opt_datetime_field(row: &Map<String, Value>, key: &str) -> Option<DateTime<Utc>> {
    row.get(key)
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn fmt_db_time(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn table_name(full_name: &str) -> String {
    let mut parts = full_name.split('.');
    let db = parts.next().unwrap_or("philand");
    let table = parts.next().unwrap_or(full_name);
    format!("`{db}`.`{table}`")
}
