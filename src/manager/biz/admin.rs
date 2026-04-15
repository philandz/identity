use tonic::Status;

use crate::converters::{base_status_from_db, user_type_from_db};
use crate::manager::biz::authz::Permission;
use crate::manager::repository::ListQuery;
use crate::manager::validate;
use crate::pb::common::base::BaseStatus;
use crate::pb::service::identity::{
    CreateOrganizationAdminResponse, CreateUserResponse, DeleteOrganizationAdminResponse,
    DeleteUserResponse, GetOrganizationAdminResponse, GetUserResponse,
    ListOrganizationsAdminResponse, ListUsersResponse, PageMeta, UpdateOrganizationAdminResponse,
    UpdateUserResponse,
};
use crate::pb::shared::organization::MemberStatus;
use crate::pb::shared::user::UserType;

use super::IdentityBiz;

fn build_list_query(
    params: Option<&crate::pb::service::identity::ListParams>,
    user_type_filter: Option<&str>,
) -> ListQuery {
    let default = crate::pb::service::identity::ListParams::default();
    let p = params.unwrap_or(&default);
    ListQuery {
        search: p.query.clone().filter(|s| !s.is_empty()),
        status: p.status.clone().filter(|s| !s.is_empty()),
        user_type: user_type_filter
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty()),
        sort_by: p.sort_by.clone().filter(|s| !s.is_empty()),
        sort_dir: p.sort_dir.clone().filter(|s| !s.is_empty()),
        page: p.page.unwrap_or(1).max(1),
        page_size: p.page_size.unwrap_or(20).clamp(1, 100),
    }
}

fn make_page_meta(page: i32, page_size: i32, total_rows: i64) -> PageMeta {
    let total_pages = ((total_rows as f64) / (page_size as f64)).ceil() as i32;
    PageMeta {
        page,
        page_size,
        total_pages: total_pages.max(1),
        total_rows,
    }
}

impl IdentityBiz {
    // -----------------------------------------------------------------------
    // Users
    // -----------------------------------------------------------------------

    pub async fn list_users(
        &self,
        caller_id: &str,
        params: Option<&crate::pb::service::identity::ListParams>,
        user_type_filter: Option<&str>,
    ) -> Result<ListUsersResponse, Status> {
        self.require_permission(caller_id, Permission::ManageAnyUser)
            .await?;

        let q = build_list_query(params, user_type_filter);
        let page = q.page;
        let page_size = q.page_size;

        let (rows, total) = self
            .repo
            .list_users_paged(&q)
            .await
            .map_err(Self::map_internal_error)?;

        let users = rows.into_iter().map(Into::into).collect();

        Ok(ListUsersResponse {
            users,
            meta: Some(make_page_meta(page, page_size, total)),
        })
    }

    pub async fn get_user(
        &self,
        caller_id: &str,
        user_id: &str,
    ) -> Result<GetUserResponse, Status> {
        self.require_permission(caller_id, Permission::ManageAnyUser)
            .await?;
        validate::non_empty_id("user_id", user_id)?;

        let user = self
            .repo
            .find_user_by_id(user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("User not found"))?;

        Ok(GetUserResponse {
            user: Some(user.into()),
        })
    }

    pub async fn create_user(
        &self,
        caller_id: &str,
        email: &str,
        password: &str,
        display_name: &str,
        user_type: Option<i32>,
    ) -> Result<CreateUserResponse, Status> {
        self.require_permission(caller_id, Permission::ManageAnyUser)
            .await?;
        validate::register_input(email, password, display_name)?;

        if let Some(v) = user_type {
            let parsed = UserType::try_from(v).unwrap_or(UserType::UtNone);
            if !matches!(parsed, UserType::UtNormal | UserType::UtSuperAdmin) {
                return Err(Status::invalid_argument(
                    "user_type must be normal or super_admin",
                ));
            }
        }

        if self
            .repo
            .find_user_by_email(email)
            .await
            .map_err(Self::map_internal_error)?
            .is_some()
        {
            return Err(Status::already_exists("Email already registered"));
        }

        let user_id = uuid::Uuid::new_v4().to_string();
        let org_id = uuid::Uuid::new_v4().to_string();
        let password_hash =
            philand_crypto::hash_password(password).map_err(Self::map_internal_error)?;

        let resolved_user_type = user_type
            .and_then(|v| UserType::try_from(v).ok())
            .unwrap_or(UserType::UtNormal);

        let db_user = self
            .repo
            .create_user_with_default_organization(
                &user_id,
                email,
                &password_hash,
                display_name,
                resolved_user_type,
                BaseStatus::BsActive,
                &org_id,
                &format!("{display_name}'s Organization"),
                crate::pb::shared::organization::OrgRole::OrOwner,
                MemberStatus::MsActive,
            )
            .await
            .map_err(Self::map_internal_error)?;

        Ok(CreateUserResponse {
            user: Some(db_user.into()),
        })
    }

    pub async fn update_user(
        &self,
        caller_id: &str,
        user_id: &str,
        display_name: Option<&str>,
        user_type: Option<i32>,
        status: Option<i32>,
    ) -> Result<UpdateUserResponse, Status> {
        self.require_permission(caller_id, Permission::ManageAnyUser)
            .await?;
        validate::admin_update_user_input(user_id, display_name, user_type, status)?;

        let existing = self
            .repo
            .find_user_by_id(user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("User not found"))?;

        let next_user_type = user_type
            .map(|v| UserType::try_from(v).unwrap_or(UserType::UtNone))
            .unwrap_or_else(|| user_type_from_db(&existing.user_type));
        let next_status = status
            .map(|v| BaseStatus::try_from(v).unwrap_or(BaseStatus::BsUnknown))
            .unwrap_or_else(|| base_status_from_db(&existing.status));

        // Guard: cannot demote/disable the last active super admin.
        let is_current_super_admin =
            user_type_from_db(&existing.user_type) == UserType::UtSuperAdmin;
        let is_current_active = base_status_from_db(&existing.status) == BaseStatus::BsActive;
        let is_demoting_or_disabling = is_current_super_admin
            && is_current_active
            && (next_user_type != UserType::UtSuperAdmin || next_status != BaseStatus::BsActive);

        if is_demoting_or_disabling {
            let count = self
                .repo
                .count_active_super_admin_users()
                .await
                .map_err(Self::map_internal_error)?;
            if count <= 1 {
                return Err(Status::failed_precondition(
                    "Cannot demote or disable the last active super admin",
                ));
            }
        }

        self.repo
            .update_user_admin(
                user_id,
                display_name,
                user_type.map(|_| next_user_type),
                status.map(|_| next_status),
            )
            .await
            .map_err(Self::map_internal_error)?;

        let user = self
            .repo
            .find_user_by_id(user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("User not found"))?;

        Ok(UpdateUserResponse {
            user: Some(user.into()),
        })
    }

    pub async fn delete_user(
        &self,
        caller_id: &str,
        user_id: &str,
    ) -> Result<DeleteUserResponse, Status> {
        self.require_permission(caller_id, Permission::ManageAnyUser)
            .await?;
        validate::non_empty_id("user_id", user_id)?;

        if caller_id == user_id {
            return Err(Status::failed_precondition(
                "Cannot delete your own account",
            ));
        }

        let existing = self
            .repo
            .find_user_by_id(user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("User not found"))?;

        // Guard: cannot delete the last active super admin.
        if user_type_from_db(&existing.user_type) == UserType::UtSuperAdmin
            && base_status_from_db(&existing.status) == BaseStatus::BsActive
        {
            let count = self
                .repo
                .count_active_super_admin_users()
                .await
                .map_err(Self::map_internal_error)?;
            if count <= 1 {
                return Err(Status::failed_precondition(
                    "Cannot delete the last active super admin",
                ));
            }
        }

        self.repo
            .delete_user(user_id)
            .await
            .map_err(Self::map_internal_error)?;

        Ok(DeleteUserResponse {})
    }

    // -----------------------------------------------------------------------
    // Organizations
    // -----------------------------------------------------------------------

    pub async fn list_organizations_admin(
        &self,
        caller_id: &str,
        params: Option<&crate::pb::service::identity::ListParams>,
    ) -> Result<ListOrganizationsAdminResponse, Status> {
        self.require_permission(caller_id, Permission::ManageAnyOrganization)
            .await?;

        let q = build_list_query(params, None);
        let page = q.page;
        let page_size = q.page_size;

        let (rows, total) = self
            .repo
            .list_organizations_paged(&q)
            .await
            .map_err(Self::map_internal_error)?;

        let organizations = rows.into_iter().map(Into::into).collect();

        Ok(ListOrganizationsAdminResponse {
            organizations,
            meta: Some(make_page_meta(page, page_size, total)),
        })
    }

    pub async fn get_organization_admin(
        &self,
        caller_id: &str,
        org_id: &str,
    ) -> Result<GetOrganizationAdminResponse, Status> {
        self.require_permission(caller_id, Permission::ManageAnyOrganization)
            .await?;
        validate::non_empty_id("org_id", org_id)?;

        let org = self
            .repo
            .find_organization_by_id(org_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("Organization not found"))?;

        Ok(GetOrganizationAdminResponse {
            organization: Some(org.into()),
        })
    }

    pub async fn create_organization_admin(
        &self,
        caller_id: &str,
        name: &str,
        owner_user_id: &str,
    ) -> Result<CreateOrganizationAdminResponse, Status> {
        self.require_permission(caller_id, Permission::ManageAnyOrganization)
            .await?;
        validate::admin_create_organization_input(name, owner_user_id)?;

        // Verify owner exists.
        self.repo
            .find_user_by_id(owner_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("Owner user not found"))?;

        let org_id = uuid::Uuid::new_v4().to_string();

        self.repo
            .insert_organization(&org_id, name, owner_user_id, BaseStatus::BsActive)
            .await
            .map_err(Self::map_internal_error)?;

        // Add owner as org member.
        self.repo
            .insert_organization_member(
                &org_id,
                owner_user_id,
                crate::pb::shared::organization::OrgRole::OrOwner,
                MemberStatus::MsActive,
            )
            .await
            .map_err(Self::map_internal_error)?;

        let org = self
            .repo
            .find_organization_by_id(&org_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::internal("Organization not found after insert"))?;

        Ok(CreateOrganizationAdminResponse {
            organization: Some(org.into()),
        })
    }

    pub async fn update_organization_admin(
        &self,
        caller_id: &str,
        org_id: &str,
        name: Option<&str>,
        status: Option<i32>,
    ) -> Result<UpdateOrganizationAdminResponse, Status> {
        self.require_permission(caller_id, Permission::ManageAnyOrganization)
            .await?;
        validate::admin_update_organization_input(org_id, name, status)?;

        self.repo
            .find_organization_by_id(org_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("Organization not found"))?;

        self.repo
            .update_organization_admin(
                org_id,
                name,
                status.map(|v| BaseStatus::try_from(v).unwrap_or(BaseStatus::BsUnknown)),
            )
            .await
            .map_err(Self::map_internal_error)?;

        let org = self
            .repo
            .find_organization_by_id(org_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("Organization not found"))?;

        Ok(UpdateOrganizationAdminResponse {
            organization: Some(org.into()),
        })
    }

    pub async fn delete_organization_admin(
        &self,
        caller_id: &str,
        org_id: &str,
    ) -> Result<DeleteOrganizationAdminResponse, Status> {
        self.require_permission(caller_id, Permission::ManageAnyOrganization)
            .await?;
        validate::non_empty_id("org_id", org_id)?;

        self.repo
            .find_organization_by_id(org_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("Organization not found"))?;

        self.repo
            .delete_organization(org_id)
            .await
            .map_err(Self::map_internal_error)?;

        Ok(DeleteOrganizationAdminResponse {})
    }
}
