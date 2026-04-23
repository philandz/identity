use crate::converters::{
    base_status_to_db, invitation_status_from_db, invitation_status_to_db, member_status_from_db,
    member_status_to_db, org_role_from_db, org_role_to_db, user_type_to_db, DbOrganization,
    DbPasswordResetToken, DbUser, DbUserOrganization,
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

/// Parameters for paginated list queries.
#[derive(Debug, Default)]
pub struct ListQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub user_type: Option<String>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub page: i32,
    pub page_size: i32,
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
        let mut migrator = sqlx::migrate::Migrator::new(std::path::Path::new("./migrations"))
            .await
            .map_err(|e| philand_storage::StorageError::Sqlx(e.into()))?;
        migrator.set_ignore_missing(true);
        migrator
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

    pub async fn list_users(&self) -> Result<Vec<DbUser>, sqlx::Error> {
        let users = table_name(philand_table::table::USERS);
        sqlx::query_as::<_, DbUser>(&format!("SELECT * FROM {users} ORDER BY created_at DESC"))
            .fetch_all(&*self.pool)
            .await
    }

    /// Paginated, filtered, sorted user list. Returns (rows, total_count).
    pub async fn list_users_paged(&self, q: &ListQuery) -> Result<(Vec<DbUser>, i64), sqlx::Error> {
        let users = table_name(philand_table::table::USERS);

        // Build WHERE clauses
        let mut conditions: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(ref search) = q.search {
            conditions.push("(display_name LIKE ? OR email LIKE ?)".to_string());
            let pattern = format!("%{search}%");
            binds.push(pattern.clone());
            binds.push(pattern);
        }
        if let Some(ref status) = q.status {
            conditions.push("status = ?".to_string());
            binds.push(status.clone());
        }
        if let Some(ref user_type) = q.user_type {
            conditions.push("user_type = ?".to_string());
            binds.push(user_type.clone());
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sort_col = match q.sort_by.as_deref() {
            Some("email") => "email",
            Some("display_name") => "display_name",
            Some("user_type") => "user_type",
            Some("status") => "status",
            _ => "created_at",
        };
        let sort_dir = if q.sort_dir.as_deref() == Some("desc") {
            "DESC"
        } else {
            "ASC"
        };
        let limit = q.page_size.clamp(1, 100) as i64;
        let offset = ((q.page.max(1) - 1) as i64) * limit;

        // Count query
        let count_sql = format!("SELECT COUNT(*) as count FROM {users} {where_clause}");
        let mut count_q = sqlx::query(&count_sql);
        for b in &binds {
            count_q = count_q.bind(b);
        }
        let total: i64 = count_q.fetch_one(&*self.pool).await?.try_get("count")?;

        // Data query
        let data_sql = format!(
            "SELECT * FROM {users} {where_clause} ORDER BY {sort_col} {sort_dir} LIMIT ? OFFSET ?"
        );
        let mut data_q = sqlx::query_as::<_, DbUser>(&data_sql);
        for b in &binds {
            data_q = data_q.bind(b);
        }
        data_q = data_q.bind(limit).bind(offset);
        let rows = data_q.fetch_all(&*self.pool).await?;

        Ok((rows, total))
    }

    pub async fn count_active_super_admin_users(&self) -> Result<i64, sqlx::Error> {
        let users = table_name(philand_table::table::USERS);
        let row = sqlx::query(&format!(
            "SELECT COUNT(*) as count FROM {users} WHERE user_type = ? AND status = ?"
        ))
        .bind(user_type_to_db(UserType::UtSuperAdmin))
        .bind(base_status_to_db(BaseStatus::BsActive))
        .fetch_one(&*self.pool)
        .await?;

        row.try_get("count")
    }

    pub async fn update_user_admin(
        &self,
        user_id: &str,
        display_name: Option<&str>,
        user_type: Option<UserType>,
        status: Option<BaseStatus>,
    ) -> Result<(), sqlx::Error> {
        let mut data = Map::new();

        if let Some(v) = display_name {
            data.insert(
                "display_name".to_string(),
                Value::String(v.trim().to_string()),
            );
        }

        if let Some(v) = user_type {
            data.insert(
                "user_type".to_string(),
                Value::String(user_type_to_db(v).to_string()),
            );
        }

        if let Some(v) = status {
            data.insert(
                "status".to_string(),
                Value::String(base_status_to_db(v).to_string()),
            );
        }

        if data.is_empty() {
            return Ok(());
        }

        let mut filters = Map::new();
        filters.insert("id".to_string(), Value::String(user_id.to_string()));
        self.inner
            .update(philand_table::table::USERS, &data, &filters)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
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
    ) -> Result<Vec<DbUserOrganization>, sqlx::Error> {
        let org_members = table_name(philand_table::table::ORGANIZATION_MEMBERS);
        let organizations = table_name(philand_table::table::ORGANIZATIONS);
        let active_status = member_status_to_db(MemberStatus::MsActive);

        sqlx::query_as::<_, DbUserOrganization>(&format!(
            "SELECT o.id, o.name, om.org_role \
             FROM {organizations} o \
             INNER JOIN {org_members} om ON o.id = om.org_id \
             WHERE om.user_id = ? AND om.status = ?"
        ))
        .bind(user_id)
        .bind(active_status)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn list_organizations(&self) -> Result<Vec<DbOrganization>, sqlx::Error> {
        let organizations = table_name(philand_table::table::ORGANIZATIONS);
        sqlx::query_as::<_, DbOrganization>(&format!(
            "SELECT * FROM {organizations} ORDER BY created_at DESC"
        ))
        .fetch_all(&*self.pool)
        .await
    }

    /// Paginated, filtered, sorted organization list. Returns (rows, total_count).
    pub async fn list_organizations_paged(
        &self,
        q: &ListQuery,
    ) -> Result<(Vec<DbOrganization>, i64), sqlx::Error> {
        let organizations = table_name(philand_table::table::ORGANIZATIONS);

        let mut conditions: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(ref search) = q.search {
            conditions.push("name LIKE ?".to_string());
            binds.push(format!("%{search}%"));
        }
        if let Some(ref status) = q.status {
            conditions.push("status = ?".to_string());
            binds.push(status.clone());
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sort_col = match q.sort_by.as_deref() {
            Some("name") => "name",
            Some("status") => "status",
            _ => "created_at",
        };
        let sort_dir = if q.sort_dir.as_deref() == Some("desc") {
            "DESC"
        } else {
            "ASC"
        };
        let limit = q.page_size.clamp(1, 100) as i64;
        let offset = ((q.page.max(1) - 1) as i64) * limit;

        let count_sql = format!("SELECT COUNT(*) as count FROM {organizations} {where_clause}");
        let mut count_q = sqlx::query(&count_sql);
        for b in &binds {
            count_q = count_q.bind(b);
        }
        let total: i64 = count_q.fetch_one(&*self.pool).await?.try_get("count")?;

        let data_sql = format!(
            "SELECT * FROM {organizations} {where_clause} ORDER BY {sort_col} {sort_dir} LIMIT ? OFFSET ?"
        );
        let mut data_q = sqlx::query_as::<_, DbOrganization>(&data_sql);
        for b in &binds {
            data_q = data_q.bind(b);
        }
        data_q = data_q.bind(limit).bind(offset);
        let rows = data_q.fetch_all(&*self.pool).await?;

        Ok((rows, total))
    }

    pub async fn find_organization_by_id(
        &self,
        org_id: &str,
    ) -> Result<Option<DbOrganization>, sqlx::Error> {
        let mut filters = Map::new();
        filters.insert("id".to_string(), Value::String(org_id.to_string()));
        let row = self
            .inner
            .get(philand_table::table::ORGANIZATIONS, &filters)
            .await
            .map_err(map_storage_error)?;
        Ok(row.map(map_to_db_organization))
    }

    pub async fn update_organization_admin(
        &self,
        org_id: &str,
        name: Option<&str>,
        status: Option<BaseStatus>,
    ) -> Result<(), sqlx::Error> {
        let mut data = Map::new();

        if let Some(v) = name {
            data.insert("name".to_string(), Value::String(v.trim().to_string()));
        }

        if let Some(v) = status {
            data.insert(
                "status".to_string(),
                Value::String(base_status_to_db(v).to_string()),
            );
        }

        if data.is_empty() {
            return Ok(());
        }

        let mut filters = Map::new();
        filters.insert("id".to_string(), Value::String(org_id.to_string()));
        self.inner
            .update(philand_table::table::ORGANIZATIONS, &data, &filters)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
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

    pub async fn update_user_profile(
        &self,
        user_id: &str,
        display_name: Option<&str>,
        avatar: Option<&str>,
        bio: Option<&str>,
        timezone: Option<&str>,
        locale: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let mut data = Map::new();

        if let Some(v) = display_name {
            data.insert(
                "display_name".to_string(),
                Value::String(v.trim().to_string()),
            );
        }

        if let Some(v) = avatar {
            if v.trim().is_empty() {
                data.insert("avatar".to_string(), Value::Null);
            } else {
                data.insert("avatar".to_string(), Value::String(v.trim().to_string()));
            }
        }

        if let Some(v) = bio {
            if v.trim().is_empty() {
                data.insert("bio".to_string(), Value::Null);
            } else {
                data.insert("bio".to_string(), Value::String(v.trim().to_string()));
            }
        }

        if let Some(v) = timezone {
            data.insert("timezone".to_string(), Value::String(v.trim().to_string()));
        }

        if let Some(v) = locale {
            data.insert("locale".to_string(), Value::String(v.trim().to_string()));
        }

        if data.is_empty() {
            return Ok(());
        }

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

    pub async fn transfer_org_ownership(
        &self,
        org_id: &str,
        current_owner_id: &str,
        new_owner_id: &str,
    ) -> Result<(), sqlx::Error> {
        let members = table_name(philand_table::table::ORGANIZATION_MEMBERS);
        let organizations = table_name(philand_table::table::ORGANIZATIONS);

        let mut tx = self.pool.begin().await?;

        // Demote current owner to admin.
        sqlx::query(&format!(
            "UPDATE {members} SET org_role = ? WHERE org_id = ? AND user_id = ?"
        ))
        .bind(org_role_to_db(OrgRole::OrAdmin))
        .bind(org_id)
        .bind(current_owner_id)
        .execute(&mut *tx)
        .await?;

        // Promote new owner.
        sqlx::query(&format!(
            "UPDATE {members} SET org_role = ? WHERE org_id = ? AND user_id = ?"
        ))
        .bind(org_role_to_db(OrgRole::OrOwner))
        .bind(org_id)
        .bind(new_owner_id)
        .execute(&mut *tx)
        .await?;

        // Update the canonical owner reference on the org row.
        sqlx::query(&format!(
            "UPDATE {organizations} SET owner_user_id = ? WHERE id = ?"
        ))
        .bind(new_owner_id)
        .bind(org_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn rename_organization(
        &self,
        org_id: &str,
        new_name: &str,
    ) -> Result<u64, sqlx::Error> {
        let mut data = Map::new();
        data.insert(
            "name".to_string(),
            Value::String(new_name.trim().to_string()),
        );
        let mut filters = Map::new();
        filters.insert("id".to_string(), Value::String(org_id.to_string()));
        self.inner
            .update(philand_table::table::ORGANIZATIONS, &data, &filters)
            .await
            .map_err(map_storage_error)
    }

    pub async fn find_pending_invitations_by_org(
        &self,
        org_id: &str,
    ) -> Result<Vec<OrganizationInvitationRow>, sqlx::Error> {
        let invitations = table_name(philand_table::table::ORGANIZATION_INVITATIONS);
        let pending = invitation_status_to_db(InvitationStatus::IsPending);

        let rows = sqlx::query(&format!(
            "SELECT id, org_id, inviter_id, invitee_email, org_role, status, expires_at, created_at \
             FROM {invitations} \
             WHERE org_id = ? AND status = ? AND expires_at > NOW() \
             ORDER BY created_at DESC"
        ))
        .bind(org_id)
        .bind(pending)
        .fetch_all(&*self.pool)
        .await?;

        rows.into_iter()
            .map(|row| -> Result<OrganizationInvitationRow, sqlx::Error> {
                Ok(OrganizationInvitationRow {
                    id: row.try_get("id")?,
                    org_id: row.try_get("org_id")?,
                    inviter_id: row.try_get("inviter_id")?,
                    invitee_email: row.try_get("invitee_email")?,
                    org_role: org_role_from_db(&row.try_get::<String, _>("org_role")?),
                    status: invitation_status_from_db(&row.try_get::<String, _>("status")?),
                    expires_at: row
                        .try_get::<DateTime<Utc>, _>("expires_at")
                        .map(|dt| dt.timestamp())
                        .unwrap_or(0),
                    created_at: row
                        .try_get::<DateTime<Utc>, _>("created_at")
                        .map(|dt| dt.timestamp())
                        .unwrap_or(0),
                })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    pub async fn revoke_invitation(
        &self,
        org_id: &str,
        invitation_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let invitations = table_name(philand_table::table::ORGANIZATION_INVITATIONS);
        let result = sqlx::query(&format!(
            "UPDATE {invitations} SET status = ? WHERE id = ? AND org_id = ? AND status = ?"
        ))
        .bind(invitation_status_to_db(InvitationStatus::IsRevoked))
        .bind(invitation_id)
        .bind(org_id)
        .bind(invitation_status_to_db(InvitationStatus::IsPending))
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Hard-delete a user row (admin only — caller must have already checked guards).
    pub async fn delete_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let mut filters = Map::new();
        filters.insert("id".to_string(), Value::String(user_id.to_string()));
        self.inner
            .delete(philand_table::table::USERS, &filters)
            .await
            .map(|_| ())
            .map_err(map_storage_error)
    }

    /// Hard-delete an organization row (admin only — caller must have already checked guards).
    pub async fn delete_organization(&self, org_id: &str) -> Result<(), sqlx::Error> {
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
        avatar: opt_string_field(&row, "avatar"),
        bio: opt_string_field(&row, "bio"),
        timezone: string_field(&row, "timezone"),
        locale: string_field(&row, "locale"),
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
