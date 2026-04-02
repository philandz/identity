//! REST (HTTP/JSON) handlers for the Identity service.
//!
//! These are thin wrappers that delegate to [`IdentityBiz`] — the same business
//! logic layer used by the gRPC handlers.  Each handler converts between
//! REST DTOs and the proto-generated types.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tonic::Status;
use utoipa::{OpenApi, ToSchema};

use crate::manager::biz::IdentityBiz;
use philand_error::ErrorEnvelope as ErrorResponse;

// ---------------------------------------------------------------------------
// Shared app state
// ---------------------------------------------------------------------------

/// Shared state passed to every HTTP handler via axum's `State` extractor.
pub type HttpState = Arc<IdentityBiz>;

// ---------------------------------------------------------------------------
// Error envelope (spec: { code, message, details })
// ---------------------------------------------------------------------------

fn map_status(status: &Status) -> (StatusCode, Json<ErrorResponse>) {
    let (http_code, envelope) = philand_error::http_error_from_tonic_status(status);
    (http_code, Json(envelope))
}

// ---------------------------------------------------------------------------
// Base DTO — mirrors common.base.Base (embedded in every domain entity)
// ---------------------------------------------------------------------------

/// Standard base fields present on every domain entity (mirrors `common.base.Base`).
#[derive(Serialize, ToSchema)]
pub struct BaseResponse {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: i64,
    pub created_by: String,
    pub updated_by: String,
    pub owner_id: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// DTOs — Register
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct RegisterRequest {
    /// User email address
    pub email: String,
    /// Password (min 8 characters)
    pub password: String,
    /// Display name
    pub display_name: String,
}

#[derive(Serialize, ToSchema)]
pub struct UserResponse {
    /// Standard base fields (id, timestamps, audit trail, status)
    pub base: BaseResponse,
    pub email: String,
    pub display_name: String,
    pub user_type: String,
}

#[derive(Serialize, ToSchema)]
pub struct RegisterResponse {
    pub user: UserResponse,
}

// ---------------------------------------------------------------------------
// DTOs — Login
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    /// User email address
    pub email: String,
    /// Password
    pub password: String,
}

#[derive(Serialize, ToSchema)]
pub struct OrgSummary {
    pub id: String,
    pub name: String,
    pub role: String,
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponse {
    pub access_token: String,
    pub user_type: String,
    pub organizations: Vec<OrgSummary>,
}

// ---------------------------------------------------------------------------
// DTOs — Logout
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct LogoutResponse {
    pub message: String,
}

// ---------------------------------------------------------------------------
// DTOs — Refresh Token
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct RefreshTokenResponse {
    pub access_token: String,
}

// ---------------------------------------------------------------------------
// DTOs — Change Password
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    /// Current password for verification
    pub current_password: String,
    /// New password (min 8 characters)
    pub new_password: String,
}

#[derive(Serialize, ToSchema)]
pub struct ChangePasswordResponse {
    pub message: String,
}

// ---------------------------------------------------------------------------
// DTOs — Forgot Password
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct ForgotPasswordRequest {
    /// Email address of the account
    pub email: String,
}

#[derive(Serialize, ToSchema)]
pub struct ForgotPasswordResponse {
    pub message: String,
}

// ---------------------------------------------------------------------------
// DTOs — Reset Password
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct ResetPasswordRequest {
    /// Reset token from the forgot-password email
    pub token: String,
    /// New password (min 8 characters)
    pub new_password: String,
}

#[derive(Serialize, ToSchema)]
pub struct ResetPasswordResponse {
    pub message: String,
}

// ---------------------------------------------------------------------------
// DTOs — Organization IAM (P1)
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct OrgMemberResponse {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub status: String,
    pub joined_at: i64,
}

#[derive(Serialize, ToSchema)]
pub struct ListOrgMembersResponse {
    pub members: Vec<OrgMemberResponse>,
}

#[derive(Deserialize, ToSchema)]
pub struct InviteMemberRequest {
    pub invitee_email: String,
    pub org_role: String,
}

#[derive(Serialize, ToSchema)]
pub struct InviteMemberResponse {
    pub invitation_id: String,
    pub invitee_email: String,
    pub org_role: String,
    pub status: String,
    pub expires_at: i64,
    pub invite_token: String,
}

#[derive(Serialize, ToSchema)]
pub struct AcceptInvitationResponse {
    pub org_id: String,
    pub role: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ChangeOrgMemberRoleRequest {
    pub org_role: String,
}

#[derive(Serialize, ToSchema)]
pub struct SimpleMessageResponse {
    pub message: String,
}

// ---------------------------------------------------------------------------
// DTOs — Profile
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct GetProfileResponse {
    pub user: UserResponse,
}

// ---------------------------------------------------------------------------
// DTOs — Organizations
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct OrgResponse {
    /// Standard base fields (id, timestamps, audit trail, owner_id, status)
    pub base: BaseResponse,
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub struct ListOrganizationsResponse {
    pub organizations: Vec<OrgResponse>,
}

// ---------------------------------------------------------------------------
// Helpers: proto → REST DTO conversions
// ---------------------------------------------------------------------------

/// Convert a proto i32 user_type to its REST label string via the centralized converter.
fn user_type_label(val: i32) -> String {
    use crate::converters::user_type_to_db;
    use crate::pb::shared::user::UserType;
    let ut = UserType::try_from(val).unwrap_or(UserType::UtNone);
    user_type_to_db(ut).to_string()
}

/// Convert a proto i32 org_role to its REST label string via the centralized converter.
fn org_role_label(val: i32) -> String {
    use crate::converters::org_role_to_db;
    use crate::pb::shared::organization::OrgRole;
    let role = OrgRole::try_from(val).unwrap_or(OrgRole::OrNone);
    org_role_to_db(role).to_string()
}

/// Convert a proto i32 status to its REST label string via the centralized converter.
fn base_status_label(val: i32) -> String {
    use crate::converters::base_status_to_db;
    use crate::pb::common::base::BaseStatus;
    let status = BaseStatus::try_from(val).unwrap_or(BaseStatus::BsUnknown);
    base_status_to_db(status).to_string()
}

fn member_status_label(val: i32) -> String {
    use crate::converters::member_status_to_db;
    use crate::pb::shared::organization::MemberStatus;
    let status = MemberStatus::try_from(val).unwrap_or(MemberStatus::MsNone);
    member_status_to_db(status).to_string()
}

fn parse_org_role_label(value: &str) -> Result<i32, (StatusCode, Json<ErrorResponse>)> {
    use crate::pb::shared::organization::OrgRole;

    let role = match value.trim().to_lowercase().as_str() {
        "owner" => OrgRole::OrOwner,
        "admin" => OrgRole::OrAdmin,
        "member" => OrgRole::OrMember,
        _ => {
            return Err(map_status(&Status::invalid_argument(
                "org_role must be one of: owner, admin, member",
            )));
        }
    };
    Ok(role as i32)
}

fn proto_base_to_rest(base: Option<&crate::pb::common::base::Base>) -> BaseResponse {
    match base {
        Some(b) => BaseResponse {
            id: b.id.clone(),
            created_at: b.created_at,
            updated_at: b.updated_at,
            deleted_at: b.deleted_at,
            created_by: b.created_by.clone(),
            updated_by: b.updated_by.clone(),
            owner_id: b.owner_id.clone(),
            status: base_status_label(b.status),
        },
        None => BaseResponse {
            id: String::new(),
            created_at: 0,
            updated_at: 0,
            deleted_at: 0,
            created_by: String::new(),
            updated_by: String::new(),
            owner_id: String::new(),
            status: "unknown".to_string(),
        },
    }
}

fn proto_user_to_rest(u: &crate::pb::shared::user::User) -> UserResponse {
    UserResponse {
        base: proto_base_to_rest(u.base.as_ref()),
        email: u.email.clone(),
        display_name: u.display_name.clone(),
        user_type: user_type_label(u.user_type),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Register a new user (creates a default organization).
#[utoipa::path(
    post,
    path = "/register",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "User registered", body = RegisterResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 409, description = "Email already exists", body = ErrorResponse),
    ),
    tag = "auth"
)]
async fn register(
    State(biz): State<HttpState>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), (StatusCode, Json<ErrorResponse>)> {
    let proto_resp = biz
        .register(&body.email, &body.password, &body.display_name)
        .await
        .map_err(|e| map_status(&e))?;

    let user = proto_resp
        .user
        .as_ref()
        .map(proto_user_to_rest)
        .unwrap_or_else(|| UserResponse {
            base: proto_base_to_rest(None),
            email: String::new(),
            display_name: String::new(),
            user_type: String::new(),
        });

    Ok((StatusCode::CREATED, Json(RegisterResponse { user })))
}

/// Authenticate and receive a JWT access token.
#[utoipa::path(
    post,
    path = "/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse),
    ),
    tag = "auth"
)]
async fn login(
    State(biz): State<HttpState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    let proto_resp = biz
        .login(&body.email, &body.password)
        .await
        .map_err(|e| map_status(&e))?;

    let organizations = proto_resp
        .organizations
        .iter()
        .map(|o| OrgSummary {
            id: o.id.clone(),
            name: o.name.clone(),
            role: org_role_label(o.role),
        })
        .collect();

    Ok(Json(LoginResponse {
        access_token: proto_resp.access_token,
        user_type: user_type_label(proto_resp.user_type),
        organizations,
    }))
}

/// Get the authenticated user's profile.
///
/// Requires `Authorization: Bearer <token>` header.
#[utoipa::path(
    get,
    path = "/profile",
    responses(
        (status = 200, description = "User profile", body = GetProfileResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
    ),
    security(("bearer_auth" = [])),
    tag = "profile"
)]
async fn get_profile(
    State(biz): State<HttpState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<GetProfileResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id_from_jwt(&biz, &headers).await?;

    let proto_resp = biz
        .get_profile(&user_id)
        .await
        .map_err(|e| map_status(&e))?;

    let user = proto_resp
        .user
        .as_ref()
        .map(proto_user_to_rest)
        .unwrap_or_else(|| UserResponse {
            base: proto_base_to_rest(None),
            email: String::new(),
            display_name: String::new(),
            user_type: String::new(),
        });

    Ok(Json(GetProfileResponse { user }))
}

/// List organizations the authenticated user belongs to.
///
/// Requires `Authorization: Bearer <token>` header.
#[utoipa::path(
    get,
    path = "/organizations",
    responses(
        (status = 200, description = "User organizations", body = ListOrganizationsResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    ),
    security(("bearer_auth" = [])),
    tag = "organizations"
)]
async fn list_organizations(
    State(biz): State<HttpState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ListOrganizationsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id_from_jwt(&biz, &headers).await?;

    let proto_resp = biz
        .list_organizations(&user_id)
        .await
        .map_err(|e| map_status(&e))?;

    let organizations = proto_resp
        .organizations
        .iter()
        .map(|o| OrgResponse {
            base: proto_base_to_rest(o.base.as_ref()),
            name: o.name.clone(),
        })
        .collect();

    Ok(Json(ListOrganizationsResponse { organizations }))
}

/// Logout the current user (revoke the JWT).
///
/// Requires `Authorization: Bearer <token>` header.
#[utoipa::path(
    post,
    path = "/logout",
    responses(
        (status = 200, description = "Logged out", body = LogoutResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    ),
    security(("bearer_auth" = [])),
    tag = "auth"
)]
async fn logout(
    State(biz): State<HttpState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<LogoutResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (token, claims) = extract_token_and_claims(&biz, &headers).await?;

    biz.logout(&token, &claims.sub, claims.exp)
        .await
        .map_err(|e| map_status(&e))?;

    Ok(Json(LogoutResponse {
        message: "Logged out successfully".to_string(),
    }))
}

/// Refresh the current JWT (issues new token, revokes old one).
///
/// Requires `Authorization: Bearer <token>` header.
#[utoipa::path(
    post,
    path = "/refresh",
    responses(
        (status = 200, description = "Token refreshed", body = RefreshTokenResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    ),
    security(("bearer_auth" = [])),
    tag = "auth"
)]
async fn refresh_token(
    State(biz): State<HttpState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<RefreshTokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (token, claims) = extract_token_and_claims(&biz, &headers).await?;

    let proto_resp = biz
        .refresh_token(&token, &claims.sub, claims.exp)
        .await
        .map_err(|e| map_status(&e))?;

    Ok(Json(RefreshTokenResponse {
        access_token: proto_resp.access_token,
    }))
}

/// Change password for the authenticated user.
///
/// Requires `Authorization: Bearer <token>` header.
#[utoipa::path(
    post,
    path = "/update",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed", body = ChangePasswordResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Invalid current password or token", body = ErrorResponse),
    ),
    security(("bearer_auth" = [])),
    tag = "auth"
)]
async fn change_password(
    State(biz): State<HttpState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id_from_jwt(&biz, &headers).await?;

    biz.change_password(&user_id, &body.current_password, &body.new_password)
        .await
        .map_err(|e| map_status(&e))?;

    Ok(Json(ChangePasswordResponse {
        message: "Password changed successfully".to_string(),
    }))
}

/// Initiate a password reset flow (sends reset token).
///
/// Public endpoint — always returns success to prevent email enumeration.
#[utoipa::path(
    post,
    path = "/forgot",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 200, description = "Reset initiated", body = ForgotPasswordResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
    ),
    tag = "auth"
)]
async fn forgot_password(
    State(biz): State<HttpState>,
    Json(body): Json<ForgotPasswordRequest>,
) -> Result<Json<ForgotPasswordResponse>, (StatusCode, Json<ErrorResponse>)> {
    let proto_resp = biz
        .forgot_password(&body.email)
        .await
        .map_err(|e| map_status(&e))?;

    Ok(Json(ForgotPasswordResponse {
        message: proto_resp.message,
    }))
}

/// Reset password using a token from the forgot-password email.
///
/// Public endpoint — validates the token and sets the new password.
#[utoipa::path(
    post,
    path = "/reset",
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset", body = ResetPasswordResponse),
        (status = 400, description = "Invalid or expired token", body = ErrorResponse),
    ),
    tag = "auth"
)]
async fn reset_password(
    State(biz): State<HttpState>,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<Json<ResetPasswordResponse>, (StatusCode, Json<ErrorResponse>)> {
    biz.reset_password(&body.token, &body.new_password)
        .await
        .map_err(|e| map_status(&e))?;

    Ok(Json(ResetPasswordResponse {
        message: "Password has been reset successfully".to_string(),
    }))
}

/// List all members of an organization.
#[utoipa::path(
    get,
    path = "/organizations/{org_id}/members",
    params(("org_id" = String, Path, description = "Organization ID")),
    responses(
        (status = 200, description = "Organization members", body = ListOrgMembersResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Not organization member", body = ErrorResponse),
    ),
    security(("bearer_auth" = [])),
    tag = "organizations"
)]
async fn list_org_members(
    State(biz): State<HttpState>,
    Path(org_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ListOrgMembersResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id_from_jwt(&biz, &headers).await?;
    let proto_resp = biz
        .list_org_members(&user_id, &org_id)
        .await
        .map_err(|e| map_status(&e))?;

    let members = proto_resp
        .members
        .into_iter()
        .map(|m| OrgMemberResponse {
            user_id: m.user_id,
            email: m.email,
            display_name: m.display_name,
            role: org_role_label(m.role),
            status: member_status_label(m.status),
            joined_at: m.joined_at,
        })
        .collect();

    Ok(Json(ListOrgMembersResponse { members }))
}

/// Invite a member to an organization.
#[utoipa::path(
    post,
    path = "/organizations/{org_id}/invitations",
    params(("org_id" = String, Path, description = "Organization ID")),
    request_body = InviteMemberRequest,
    responses(
        (status = 200, description = "Invitation created", body = InviteMemberResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Insufficient permission", body = ErrorResponse),
    ),
    security(("bearer_auth" = [])),
    tag = "organizations"
)]
async fn invite_member(
    State(biz): State<HttpState>,
    Path(org_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<InviteMemberRequest>,
) -> Result<Json<InviteMemberResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id_from_jwt(&biz, &headers).await?;
    let org_role = parse_org_role_label(&body.org_role)?;

    let proto_resp = biz
        .invite_member(&user_id, &org_id, &body.invitee_email, org_role)
        .await
        .map_err(|e| map_status(&e))?;

    let invitation = proto_resp
        .invitation
        .ok_or_else(|| map_status(&Status::internal("Invitation payload missing")))?;

    Ok(Json(InviteMemberResponse {
        invitation_id: invitation.id,
        invitee_email: invitation.invitee_email,
        org_role: org_role_label(invitation.org_role),
        status: "pending".to_string(),
        expires_at: invitation.expires_at,
        invite_token: proto_resp.invite_token,
    }))
}

/// Accept an invitation by token.
#[utoipa::path(
    post,
    path = "/invitations/{token}/accept",
    params(("token" = String, Path, description = "Invitation token")),
    responses(
        (status = 200, description = "Invitation accepted", body = AcceptInvitationResponse),
        (status = 400, description = "Invalid token", body = ErrorResponse),
    ),
    tag = "organizations"
)]
async fn accept_invitation(
    State(biz): State<HttpState>,
    Path(token): Path<String>,
) -> Result<Json<AcceptInvitationResponse>, (StatusCode, Json<ErrorResponse>)> {
    let proto_resp = biz
        .accept_invitation(&token)
        .await
        .map_err(|e| map_status(&e))?;

    Ok(Json(AcceptInvitationResponse {
        org_id: proto_resp.org_id,
        role: proto_resp.role,
    }))
}

/// Change a member role in an organization.
#[utoipa::path(
    patch,
    path = "/organizations/{org_id}/members/{user_id}/role",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("user_id" = String, Path, description = "Target user ID")
    ),
    request_body = ChangeOrgMemberRoleRequest,
    responses(
        (status = 200, description = "Role updated", body = SimpleMessageResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Insufficient permission", body = ErrorResponse),
        (status = 404, description = "Target member not found", body = ErrorResponse),
    ),
    security(("bearer_auth" = [])),
    tag = "organizations"
)]
async fn change_org_member_role(
    State(biz): State<HttpState>,
    Path((org_id, user_id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(body): Json<ChangeOrgMemberRoleRequest>,
) -> Result<Json<SimpleMessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let caller_user_id = extract_user_id_from_jwt(&biz, &headers).await?;
    let org_role = parse_org_role_label(&body.org_role)?;

    biz.change_org_member_role(&caller_user_id, &org_id, &user_id, org_role)
        .await
        .map_err(|e| map_status(&e))?;

    Ok(Json(SimpleMessageResponse {
        message: "Role updated successfully".to_string(),
    }))
}

/// Remove a member from an organization.
#[utoipa::path(
    delete,
    path = "/organizations/{org_id}/members/{user_id}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "Member removed", body = SimpleMessageResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Insufficient permission", body = ErrorResponse),
        (status = 404, description = "Target member not found", body = ErrorResponse),
    ),
    security(("bearer_auth" = [])),
    tag = "organizations"
)]
async fn remove_org_member(
    State(biz): State<HttpState>,
    Path((org_id, user_id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<SimpleMessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let caller_user_id = extract_user_id_from_jwt(&biz, &headers).await?;

    biz.remove_org_member(&caller_user_id, &org_id, &user_id)
        .await
        .map_err(|e| map_status(&e))?;

    Ok(Json(SimpleMessageResponse {
        message: "Member removed successfully".to_string(),
    }))
}

// ---------------------------------------------------------------------------
// JWT extraction helper
// ---------------------------------------------------------------------------

async fn extract_user_id_from_jwt(
    biz: &IdentityBiz,
    headers: &axum::http::HeaderMap,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| map_status(&Status::unauthenticated("Missing Authorization header")))?;

    let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        map_status(&Status::unauthenticated(
            "Authorization header must start with 'Bearer '",
        ))
    })?;

    let claims = biz.verify_jwt(token).await.map_err(|e| map_status(&e))?;
    Ok(claims.sub)
}

/// Extract the raw JWT token string from the Authorization header.
async fn extract_token_and_claims(
    biz: &IdentityBiz,
    headers: &axum::http::HeaderMap,
) -> Result<(String, crate::manager::biz::Claims), (StatusCode, Json<ErrorResponse>)> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| map_status(&Status::unauthenticated("Missing Authorization header")))?;

    let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        map_status(&Status::unauthenticated(
            "Authorization header must start with 'Bearer '",
        ))
    })?;

    let claims = biz.verify_jwt(token).await.map_err(|e| map_status(&e))?;
    Ok((token.to_string(), claims))
}

// ---------------------------------------------------------------------------
// Router + OpenAPI doc
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Philand Identity Service",
        description = "Identity microservice — authentication, profiles, organizations.",
        version = "0.1.0"
    ),
    paths(
        register,
        login,
        get_profile,
        list_organizations,
        logout,
        refresh_token,
        change_password,
        forgot_password,
        reset_password,
        list_org_members,
        invite_member,
        accept_invitation,
        change_org_member_role,
        remove_org_member
    ),
    components(schemas(
        BaseResponse,
        RegisterRequest, RegisterResponse,
        LoginRequest, LoginResponse,
        GetProfileResponse,
        ListOrganizationsResponse,
        OrgSummary, OrgResponse, UserResponse, ErrorResponse,
        LogoutResponse,
        RefreshTokenResponse,
        ChangePasswordRequest, ChangePasswordResponse,
        ForgotPasswordRequest, ForgotPasswordResponse,
        ResetPasswordRequest, ResetPasswordResponse,
        OrgMemberResponse, ListOrgMembersResponse,
        InviteMemberRequest, InviteMemberResponse,
        AcceptInvitationResponse,
        ChangeOrgMemberRoleRequest,
        SimpleMessageResponse,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "auth", description = "Authentication endpoints"),
        (name = "profile", description = "User profile endpoints"),
        (name = "organizations", description = "Organization and IAM endpoints"),
        (name = "health", description = "Health check"),
    )
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            );
        }
    }
}

/// Build the REST router for the identity service.
pub fn router() -> Router<HttpState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/refresh", post(refresh_token))
        .route("/update", post(change_password))
        .route("/forgot", post(forgot_password))
        .route("/reset", post(reset_password))
        .route("/organizations/{org_id}/members", get(list_org_members))
        .route("/organizations/{org_id}/invitations", post(invite_member))
        .route("/invitations/{token}/accept", post(accept_invitation))
        .route(
            "/organizations/{org_id}/members/{user_id}/role",
            patch(change_org_member_role),
        )
        .route(
            "/organizations/{org_id}/members/{user_id}",
            delete(remove_org_member),
        )
        .route("/profile", get(get_profile))
        .route("/organizations", get(list_organizations))
}
