use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::manager::biz::IdentityBiz;
use crate::pb::service::identity::identity_service_server::IdentityService;
use crate::pb::service::identity::{
    AcceptInvitationRequest, AcceptInvitationResponse, ChangeOrgMemberRoleRequest,
    ChangeOrgMemberRoleResponse, ChangePasswordRequest, ChangePasswordResponse,
    ForgotPasswordRequest, ForgotPasswordResponse, GetProfileRequest, GetProfileResponse,
    InviteMemberRequest, InviteMemberResponse, ListOrgMembersRequest, ListOrgMembersResponse,
    ListOrganizationsRequest, ListOrganizationsResponse, LoginRequest, LoginResponse,
    LogoutRequest, LogoutResponse, RefreshTokenRequest, RefreshTokenResponse, RegisterRequest,
    RegisterResponse, RemoveOrgMemberRequest, RemoveOrgMemberResponse, ResetPasswordRequest,
    ResetPasswordResponse,
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
}
