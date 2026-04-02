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
use sqlx::Row;

pub struct IdentityRepository {
    inner: philand_storage::repo::Repo,
    pool: std::sync::Arc<sqlx::Pool<sqlx::MySql>>,
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
        let pool = std::sync::Arc::new(
            sqlx::mysql::MySqlPoolOptions::new()
                .connect(&config.database_url)
                .await
                .map_err(philand_storage::StorageError::Sqlx)?,
        );
        let repo = Self::from_pool(pool);
        repo.ensure_tables().await?;
        Ok(repo)
    }

    pub fn from_pool(pool: std::sync::Arc<sqlx::Pool<sqlx::MySql>>) -> Self {
        Self {
            inner: philand_storage::repo::Repo::from_pool(pool.clone()),
            pool,
        }
    }

    async fn ensure_tables(&self) -> Result<(), philand_storage::StorageError> {
        sqlx::migrate!("./migrations")
            .run(&*self.pool)
            .await
            .map_err(|e| philand_storage::StorageError::Sqlx(e.into()))
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
        let users = table_name(philand_table::table::USERS);
        let organizations = table_name(philand_table::table::ORGANIZATIONS);
        let organization_members = table_name(philand_table::table::ORGANIZATION_MEMBERS);

        let mut tx = self.pool.begin().await?;

        sqlx::query(&format!(
            "INSERT INTO {users} (id, email, password_hash, display_name, user_type, status) VALUES (?, ?, ?, ?, ?, ?)"
        ))
        .bind(user_id)
        .bind(email)
        .bind(password_hash)
        .bind(display_name)
        .bind(user_type_to_db(user_type))
        .bind(base_status_to_db(user_status))
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!(
            "INSERT INTO {organizations} (id, name, owner_user_id, status) VALUES (?, ?, ?, ?)"
        ))
        .bind(org_id)
        .bind(org_name)
        .bind(user_id)
        .bind(base_status_to_db(BaseStatus::BsActive))
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!(
            "INSERT INTO {organization_members} (org_id, user_id, org_role, status) VALUES (?, ?, ?, ?)"
        ))
        .bind(org_id)
        .bind(user_id)
        .bind(org_role_to_db(org_role))
        .bind(member_status_to_db(membership_status))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

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
        let org_members = table_name(philand_table::table::ORGANIZATION_MEMBERS);
        let organizations = table_name(philand_table::table::ORGANIZATIONS);
        let active_status = member_status_to_db(MemberStatus::MsActive);

        let rows = sqlx::query(&format!(
            "SELECT om.org_id, om.org_role, o.name \
             FROM {org_members} om \
             INNER JOIN {organizations} o ON om.org_id = o.id \
             WHERE om.user_id = ? AND om.status = ?"
        ))
        .bind(user_id)
        .bind(active_status)
        .fetch_all(&*self.pool)
        .await?;

        let out = rows
            .into_iter()
            .map(|row| -> Result<OrgSummaryRow, sqlx::Error> {
                Ok(OrgSummaryRow {
                    id: row.try_get("org_id")?,
                    name: row.try_get("name")?,
                    role: org_role_from_db(&row.try_get::<String, _>("org_role")?),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(out)
    }

    pub async fn find_user_organizations(
        &self,
        user_id: &str,
    ) -> Result<Vec<DbOrganization>, sqlx::Error> {
        let org_members = table_name(philand_table::table::ORGANIZATION_MEMBERS);
        let organizations = table_name(philand_table::table::ORGANIZATIONS);
        let active_status = member_status_to_db(MemberStatus::MsActive);

        sqlx::query_as::<_, DbOrganization>(&format!(
            "SELECT o.* \
             FROM {organizations} o \
             INNER JOIN {org_members} om ON o.id = om.org_id \
             WHERE om.user_id = ? AND om.status = ?"
        ))
        .bind(user_id)
        .bind(active_status)
        .fetch_all(&*self.pool)
        .await
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
        let res = self
            .inner
            .create(philand_table::table::REVOKED_TOKENS, &data)
            .await
            .map(|_| ())
            .map_err(map_storage_error);

        match res {
            Ok(()) => Ok(()),
            // Token is already revoked — idempotent, treat as success.
            Err(e) if self.is_unique_violation(&e) => Ok(()),
            Err(e) => Err(e),
        }
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
        filters.insert(
            "status".to_string(),
            Value::String(member_status_to_db(MemberStatus::MsActive).to_string()),
        );
        let row = self
            .inner
            .get(philand_table::table::ORGANIZATION_MEMBERS, &filters)
            .await
            .map_err(map_storage_error)?;
        Ok(row.map(|r| org_role_from_db(&string_field(&r, "org_role"))))
    }

    pub async fn list_org_members(&self, org_id: &str) -> Result<Vec<OrgMemberRow>, sqlx::Error> {
        let org_members = table_name(philand_table::table::ORGANIZATION_MEMBERS);
        let users = table_name(philand_table::table::USERS);

        let rows = sqlx::query(&format!(
            "SELECT om.user_id, u.email, u.display_name, om.org_role, om.status, om.joined_at \
             FROM {org_members} om \
             INNER JOIN {users} u ON om.user_id = u.id \
             WHERE om.org_id = ? \
             ORDER BY om.joined_at ASC"
        ))
        .bind(org_id)
        .fetch_all(&*self.pool)
        .await?;

        let out = rows
            .into_iter()
            .map(|row| -> Result<OrgMemberRow, sqlx::Error> {
                Ok(OrgMemberRow {
                    user_id: row.try_get("user_id")?,
                    email: row.try_get("email")?,
                    display_name: row.try_get("display_name")?,
                    role: org_role_from_db(&row.try_get::<String, _>("org_role")?),
                    status: member_status_from_db(&row.try_get::<String, _>("status")?),
                    joined_at: row
                        .try_get::<DateTime<Utc>, _>("joined_at")
                        .map(|dt| dt.timestamp())
                        .unwrap_or(0),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(out)
    }

    pub async fn upsert_organization_invitation(
        &self,
        params: UpsertOrganizationInvitationParams<'_>,
    ) -> Result<(), sqlx::Error> {
        // Atomic upsert via INSERT … ON DUPLICATE KEY UPDATE.
        // Relies on UNIQUE KEY uk_org_email (org_id, invitee_email) in organization_invitations.
        let invitations = table_name(philand_table::table::ORGANIZATION_INVITATIONS);

        sqlx::query(&format!(
            "INSERT INTO {invitations} \
             (id, org_id, inviter_id, invitee_email, org_role, token_hash, status, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             ON DUPLICATE KEY UPDATE \
               id = VALUES(id), \
               inviter_id = VALUES(inviter_id), \
               org_role = VALUES(org_role), \
               token_hash = VALUES(token_hash), \
               status = VALUES(status), \
               expires_at = VALUES(expires_at)"
        ))
        .bind(params.id)
        .bind(params.org_id)
        .bind(params.inviter_id)
        .bind(params.invitee_email)
        .bind(org_role_to_db(params.org_role))
        .bind(params.token_hash)
        .bind(invitation_status_to_db(params.status))
        .bind(fmt_db_time(params.expires_at))
        .execute(&*self.pool)
        .await
        .map(|_| ())
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
        filters.insert(
            "status".to_string(),
            Value::String(member_status_to_db(MemberStatus::MsActive).to_string()),
        );
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
        let invitations = table_name(philand_table::table::ORGANIZATION_INVITATIONS);
        let members = table_name(philand_table::table::ORGANIZATION_MEMBERS);

        let mut tx = self.pool.begin().await?;

        sqlx::query(&format!("UPDATE {invitations} SET status = ? WHERE id = ?"))
            .bind(invitation_status_to_db(InvitationStatus::IsAccepted))
            .bind(invitation_id)
            .execute(&mut *tx)
            .await?;

        // Upsert membership atomically.
        // Relies on the PRIMARY KEY (org_id, user_id) in organization_members.
        sqlx::query(&format!(
            "INSERT INTO {members} (org_id, user_id, org_role, status) VALUES (?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE org_role = VALUES(org_role), status = VALUES(status)"
        ))
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
        let mut data = Map::new();
        data.insert(
            "org_role".to_string(),
            Value::String(org_role_to_db(new_role).to_string()),
        );
        let mut filters = Map::new();
        filters.insert("org_id".to_string(), Value::String(org_id.to_string()));
        filters.insert("user_id".to_string(), Value::String(user_id.to_string()));
        filters.insert(
            "status".to_string(),
            Value::String(member_status_to_db(MemberStatus::MsActive).to_string()),
        );
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
}

fn map_storage_error(err: philand_storage::StorageError) -> sqlx::Error {
    match err {
        philand_storage::StorageError::Sqlx(e) => e,
        // sqlx 0.7 does not expose an `Other` variant; Protocol is the closest
        // semantic fit for non-database errors surfaced through the storage layer.
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
    opt_datetime_field(row, key).unwrap_or_else(|| {
        tracing::warn!(
            "Missing or unparseable datetime field '{}'; using epoch as sentinel",
            key
        );
        DateTime::<Utc>::UNIX_EPOCH
    })
}

fn opt_datetime_field(row: &Map<String, Value>, key: &str) -> Option<DateTime<Utc>> {
    row.get(key).and_then(Value::as_str).and_then(|s| {
        // Try RFC3339 first (e.g. "2023-01-15T10:30:00+00:00").
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .ok()
            // Fall back to MySQL DATETIME format (e.g. "2023-01-15 10:30:00").
            .or_else(|| {
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|dt| dt.and_utc())
            })
    })
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
