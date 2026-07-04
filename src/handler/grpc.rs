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
    ConfirmPasswordChangeOtpRequest,
    ConfirmPasswordChangeOtpResponse,
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
    GetResendConfigRequest,
    GetResendConfigResponse,
    GetSystemConfigRequest,
    GetSystemConfigResponse,
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
    RequestPasswordChangeOtpRequest,
    RequestPasswordChangeOtpResponse,
    ResetPasswordRequest,
    ResetPasswordResponse,
    TestResendConfigRequest,
    TestResendConfigResponse,
    UpdateOrganizationAdminRequest,
    UpdateOrganizationAdminResponse,
    UpdateProfileRequest,
    UpdateProfileResponse,
    UpdateResendConfigRequest,
    UpdateResendConfigResponse,
    UpdateSystemConfigRequest,
    UpdateSystemConfigResponse,
    UpdateUserRequest,
    UpdateUserResponse,
};

use super::metadata::{extract_bearer_token, user_id_from_metadata};

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

    async fn login_with_google(
        &self,
        request: Request<crate::pb::service::identity::LoginWithGoogleRequest>,
    ) -> Result<Response<LoginResponse>, Status> {
        let req = request.into_inner();
        let resp = self.biz.login_with_google(&req.id_token).await?;
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
        let service_actor = request
            .metadata()
            .get("x-service-actor")
            .and_then(|v| v.to_str().ok())
            .map(|s| s == "true")
            .unwrap_or(false);
        if service_actor {
            let caller_id = user_id_from_metadata(request.metadata())?;
            self.biz
                .require_permission(
                    &caller_id,
                    crate::manager::biz::authz::Permission::ManageAnyOrganization,
                )
                .await?;
            let req = request.into_inner();
            // super-admin with service-actor: skip org-membership check, go straight to repo
            let members = self.biz.list_org_members_no_auth(&req.org_id).await?;
            return Ok(Response::new(members));
        }
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
        let org_role = self.biz.get_org_role(&req.user_id, &req.org_id).await?;
        Ok(Response::new(GetOrgRoleResponse {
            role: org_role as i32,
        }))
    }

    // -- Logged-in password change (mandatory email OTP) -----------------

    async fn request_password_change_otp(
        &self,
        request: Request<RequestPasswordChangeOtpRequest>,
    ) -> Result<Response<RequestPasswordChangeOtpResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let resp = self
            .biz
            .request_password_change_otp(&claims.sub, request.into_inner())
            .await?;
        Ok(Response::new(resp))
    }

    async fn confirm_password_change_otp(
        &self,
        request: Request<ConfirmPasswordChangeOtpRequest>,
    ) -> Result<Response<ConfirmPasswordChangeOtpResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let resp = self
            .biz
            .confirm_password_change_otp(&claims.sub, request.into_inner())
            .await?;
        Ok(Response::new(resp))
    }

    // -- Super-admin platform settings -----------------------------------

    async fn get_resend_config(
        &self,
        request: Request<GetResendConfigRequest>,
    ) -> Result<Response<GetResendConfigResponse>, Status> {
        let _ = extract_bearer_token(&request)?;
        let resp = self.biz.get_resend_config().await?;
        Ok(Response::new(resp))
    }

    async fn update_resend_config(
        &self,
        request: Request<UpdateResendConfigRequest>,
    ) -> Result<Response<UpdateResendConfigResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let reply_to = if req.reply_to.is_empty() {
            None
        } else {
            Some(req.reply_to.as_str())
        };
        let resp = self
            .biz
            .update_resend_config(&claims.sub, Some(&req.api_key), &req.from_address, reply_to)
            .await?;
        Ok(Response::new(resp))
    }

    async fn test_resend_config(
        &self,
        request: Request<TestResendConfigRequest>,
    ) -> Result<Response<TestResendConfigResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let resp = self
            .biz
            .test_resend_config(&claims.sub, &request.into_inner().recipient_email)
            .await?;
        Ok(Response::new(resp))
    }

    // -- Super-admin system configuration -------------------------------

    async fn get_system_config(
        &self,
        request: Request<GetSystemConfigRequest>,
    ) -> Result<Response<GetSystemConfigResponse>, Status> {
        let _ = extract_bearer_token(&request)?;
        Ok(Response::new(self.biz.get_system_config().await?))
    }

    async fn update_system_config(
        &self,
        request: Request<UpdateSystemConfigRequest>,
    ) -> Result<Response<UpdateSystemConfigResponse>, Status> {
        let token = extract_bearer_token(&request)?;
        let claims = self.biz.verify_jwt(&token).await?;
        let req = request.into_inner();
        let resp = self
            .biz
            .update_system_config(
                &claims.sub,
                req.app_public_base_url,
                req.support_email,
                req.default_locale,
                req.mail_from_address,
            )
            .await?;
        Ok(Response::new(resp))
    }
}
