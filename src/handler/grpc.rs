use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::manager::biz::IdentityBiz;
use crate::pb::service::identity::identity_service_server::IdentityService;
use crate::pb::service::identity::{
    AcceptInvitationRequest,
    AcceptInvitationResponse,
    ChangeOrgMemberRoleRequest,
    ChangeOrgMemberRoleResponse,
    ChangePasswordRequest,
    ChangePasswordResponse,
    CreateOrganizationAdminRequest,
    CreateOrganizationAdminResponse,
    CreateUserRequest,
    CreateUserResponse,
    DeleteOrganizationAdminRequest,
    DeleteOrganizationAdminResponse,
    DeleteUserRequest,
    DeleteUserResponse,
    ForgotPasswordRequest,
    ForgotPasswordResponse,
    GetOrgRoleRequest,
    GetOrgRoleResponse,
    GetOrganizationAdminRequest,
    GetOrganizationAdminResponse,
    GetProfileRequest,
    GetProfileResponse,
    GetUserRequest,
    GetUserResponse,
    InviteMemberRequest,
    InviteMemberResponse,
    ListOrgMembersRequest,
    ListOrgMembersResponse,
    ListOrganizationsAdminRequest,
    ListOrganizationsAdminResponse,
    ListOrganizationsRequest,
    ListOrganizationsResponse,
    // P2 — admin CRUD
    ListUsersRequest,
    ListUsersResponse,
    LoginRequest,
    LoginResponse,
    LogoutRequest,
    LogoutResponse,
    RefreshTokenRequest,
    RefreshTokenResponse,
    RegisterRequest,
    RegisterResponse,
    RemoveOrgMemberRequest,
    RemoveOrgMemberResponse,
    ResetPasswordRequest,
    ResetPasswordResponse,
    UpdateOrganizationAdminRequest,
    UpdateOrganizationAdminResponse,
    UpdateProfileRequest,
    UpdateProfileResponse,
    UpdateUserRequest,
    UpdateUserResponse,
};

use super::metadata::extract_bearer_token;

pub struct IdentityHandler {
    biz: Arc<IdentityBiz>,
}

impl IdentityHandler {
    pub fn new(biz: Arc<IdentityBiz>) -> Self {
        Self { biz }
    }
}

#[tonic::async_trait]
impl IdentityService for IdentityHandler {
    async fn register(
        &self,
        request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let req = request.into_inner();
        let resp = self
            .biz
            .register(&req.email, &req.password, &req.display_name)
            .await?;
        Ok(Response::new(resp))
    }

    async fn login(
        &self,
        request: Request<LoginRequest>,
    ) -> Result<Response<LoginResponse>, Status> {
        let req = request.into_inner();
        let resp = self.biz.login(&req.email, &req.password).await?;
        Ok(Response::new(resp))
    }

    async fn get_profile(
        &self,
        request: Request<GetProfileRequest>,
    ) -> Result<Response<GetProfileResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let resp = self.biz.get_profile(&claims.sub).await?;
        Ok(Response::new(resp))
    }

    async fn list_organizations(
        &self,
        request: Request<ListOrganizationsRequest>,
    ) -> Result<Response<ListOrganizationsResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let resp = self.biz.list_organizations(&claims.sub).await?;
        Ok(Response::new(resp))
    }

    async fn update_profile(
        &self,
        request: Request<UpdateProfileRequest>,
    ) -> Result<Response<UpdateProfileResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .update_profile(
                &claims.sub,
                req.display_name.as_deref(),
                req.avatar.as_deref(),
                req.bio.as_deref(),
                req.timezone.as_deref(),
                req.locale.as_deref(),
            )
            .await?;
        Ok(Response::new(resp))
    }

    async fn logout(
        &self,
        request: Request<LogoutRequest>,
    ) -> Result<Response<LogoutResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let resp = self.biz.logout(&token, &claims.sub, claims.exp).await?;
        Ok(Response::new(resp))
    }

    async fn refresh_token(
        &self,
        request: Request<RefreshTokenRequest>,
    ) -> Result<Response<RefreshTokenResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let resp = self
            .biz
            .refresh_token(&token, &claims.sub, claims.exp)
            .await?;
        Ok(Response::new(resp))
    }

    async fn change_password(
        &self,
        request: Request<ChangePasswordRequest>,
    ) -> Result<Response<ChangePasswordResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .change_password(&claims.sub, &req.current_password, &req.new_password)
            .await?;
        Ok(Response::new(resp))
    }

    async fn forgot_password(
        &self,
        request: Request<ForgotPasswordRequest>,
    ) -> Result<Response<ForgotPasswordResponse>, Status> {
        let req = request.into_inner();
        let resp = self.biz.forgot_password(&req.email).await?;
        Ok(Response::new(resp))
    }

    async fn reset_password(
        &self,
        request: Request<ResetPasswordRequest>,
    ) -> Result<Response<ResetPasswordResponse>, Status> {
        let req = request.into_inner();
        let resp = self
            .biz
            .reset_password(&req.token, &req.new_password)
            .await?;
        Ok(Response::new(resp))
    }

    async fn list_org_members(
        &self,
        request: Request<ListOrgMembersRequest>,
    ) -> Result<Response<ListOrgMembersResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self.biz.list_org_members(&claims.sub, &req.org_id).await?;
        Ok(Response::new(resp))
    }

    async fn invite_member(
        &self,
        request: Request<InviteMemberRequest>,
    ) -> Result<Response<InviteMemberResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .invite_member(&claims.sub, &req.org_id, &req.invitee_email, req.org_role)
            .await?;
        Ok(Response::new(resp))
    }

    async fn accept_invitation(
        &self,
        request: Request<AcceptInvitationRequest>,
    ) -> Result<Response<AcceptInvitationResponse>, Status> {
        let req = request.into_inner();
        let resp = self.biz.accept_invitation(&req.token).await?;
        Ok(Response::new(resp))
    }

    async fn change_org_member_role(
        &self,
        request: Request<ChangeOrgMemberRoleRequest>,
    ) -> Result<Response<ChangeOrgMemberRoleResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .change_org_member_role(&claims.sub, &req.org_id, &req.user_id, req.org_role)
            .await?;
        Ok(Response::new(resp))
    }

    async fn remove_org_member(
        &self,
        request: Request<RemoveOrgMemberRequest>,
    ) -> Result<Response<RemoveOrgMemberResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .remove_org_member(&claims.sub, &req.org_id, &req.user_id)
            .await?;
        Ok(Response::new(resp))
    }

    async fn list_users(
        &self,
        request: Request<ListUsersRequest>,
    ) -> Result<Response<ListUsersResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .list_users(&claims.sub, req.params.as_ref(), req.user_type.as_deref())
            .await?;
        Ok(Response::new(resp))
    }

    async fn get_user(
        &self,
        request: Request<GetUserRequest>,
    ) -> Result<Response<GetUserResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self.biz.get_user(&claims.sub, &req.user_id).await?;
        Ok(Response::new(resp))
    }

    async fn create_user(
        &self,
        request: Request<CreateUserRequest>,
    ) -> Result<Response<CreateUserResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let user_type = req.user_type;
        let resp = self
            .biz
            .create_user(
                &claims.sub,
                &req.email,
                &req.password,
                &req.display_name,
                user_type,
            )
            .await?;
        Ok(Response::new(resp))
    }

    async fn update_user(
        &self,
        request: Request<UpdateUserRequest>,
    ) -> Result<Response<UpdateUserResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let user_type = req.user_type;
        let status = req.status;
        let resp = self
            .biz
            .update_user(
                &claims.sub,
                &req.user_id,
                req.display_name.as_deref(),
                user_type,
                status,
            )
            .await?;
        Ok(Response::new(resp))
    }

    async fn delete_user(
        &self,
        request: Request<DeleteUserRequest>,
    ) -> Result<Response<DeleteUserResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self.biz.delete_user(&claims.sub, &req.user_id).await?;
        Ok(Response::new(resp))
    }

    async fn list_organizations_admin(
        &self,
        request: Request<ListOrganizationsAdminRequest>,
    ) -> Result<Response<ListOrganizationsAdminResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .list_organizations_admin(&claims.sub, req.params.as_ref())
            .await?;
        Ok(Response::new(resp))
    }

    async fn get_organization_admin(
        &self,
        request: Request<GetOrganizationAdminRequest>,
    ) -> Result<Response<GetOrganizationAdminResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .get_organization_admin(&claims.sub, &req.org_id)
            .await?;
        Ok(Response::new(resp))
    }

    async fn create_organization_admin(
        &self,
        request: Request<CreateOrganizationAdminRequest>,
    ) -> Result<Response<CreateOrganizationAdminResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .create_organization_admin(&claims.sub, &req.name, &req.owner_user_id)
            .await?;
        Ok(Response::new(resp))
    }

    async fn update_organization_admin(
        &self,
        request: Request<UpdateOrganizationAdminRequest>,
    ) -> Result<Response<UpdateOrganizationAdminResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let status = req.status;
        let resp = self
            .biz
            .update_organization_admin(&claims.sub, &req.org_id, req.name.as_deref(), status)
            .await?;
        Ok(Response::new(resp))
    }

    async fn delete_organization_admin(
        &self,
        request: Request<DeleteOrganizationAdminRequest>,
    ) -> Result<Response<DeleteOrganizationAdminResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .delete_organization_admin(&claims.sub, &req.org_id)
            .await?;
        Ok(Response::new(resp))
    }
    async fn get_org_role(
        &self,
        request: Request<GetOrgRoleRequest>,
    ) -> Result<Response<GetOrgRoleResponse>, Status> {
        let req = request.into_inner();
        let role = self.biz.get_org_role(&req.user_id, &req.org_id).await?;
        Ok(Response::new(GetOrgRoleResponse { role: role as i32 }))
    }
}
