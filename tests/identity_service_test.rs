use identity::config::AppConfig;
use identity::handler::IdentityHandler;
use identity::manager::biz::IdentityBiz;
use identity::manager::repository::IdentityRepository;
use identity::pb::service::identity::identity_service_client::IdentityServiceClient;
use identity::pb::service::identity::identity_service_server::IdentityServiceServer;
use identity::pb::service::identity::{
    AcceptInvitationRequest, ChangeOrgMemberRoleRequest, ChangePasswordRequest,
    ForgotPasswordRequest, GetProfileRequest, InviteMemberRequest, ListOrgMembersRequest,
    ListOrganizationsRequest, LoginRequest, LogoutRequest, RefreshTokenRequest, RegisterRequest,
    RemoveOrgMemberRequest, ResetPasswordRequest,
};
use identity::pb::shared::organization::OrgRole;
use sqlx::MySqlPool;
use std::sync::Arc;
use tonic::Request;

fn with_bearer<T>(message: T, token: &str) -> Request<T> {
    let mut req = Request::new(message);
    req.metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    req
}

/// Helper: boots a gRPC server on a random port and returns a connected client.
async fn setup() -> (IdentityServiceClient<tonic::transport::Channel>, MySqlPool) {
    dotenvy::dotenv().ok();

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    let pool = MySqlPool::connect(&database_url).await.unwrap();

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    // Clean tables
    sqlx::query("DELETE FROM organization_invitations")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM revoked_tokens")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM password_reset_tokens")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM organization_members")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM organizations")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users")
        .execute(&pool)
        .await
        .unwrap();

    let config = AppConfig::from_env();
    let repo = IdentityRepository::new(Arc::new(pool.clone()));
    let biz = Arc::new(IdentityBiz::new(repo, config));
    let handler = IdentityHandler::new(biz);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(IdentityServiceServer::new(handler))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = IdentityServiceClient::connect(format!("http://{}", addr))
        .await
        .unwrap();

    (client, pool)
}

#[tokio::test]
#[serial_test::serial]
async fn test_register_success() {
    let (mut client, _pool) = setup().await;
    let res = client
        .register(Request::new(RegisterRequest {
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
            display_name: "Test User".to_string(),
        }))
        .await;
    assert!(res.is_ok(), "Register should succeed: {:?}", res.err());
    let user = res.unwrap().into_inner().user.unwrap();
    assert_eq!(user.email, "test@example.com");
    assert_eq!(user.display_name, "Test User");
}

#[tokio::test]
#[serial_test::serial]
async fn test_register_duplicate_email() {
    let (mut client, _pool) = setup().await;
    client
        .register(Request::new(RegisterRequest {
            email: "dup@example.com".to_string(),
            password: "password123".to_string(),
            display_name: "User 1".to_string(),
        }))
        .await
        .unwrap();
    let res = client
        .register(Request::new(RegisterRequest {
            email: "dup@example.com".to_string(),
            password: "password456".to_string(),
            display_name: "User 2".to_string(),
        }))
        .await;
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), tonic::Code::AlreadyExists);
}

#[tokio::test]
#[serial_test::serial]
async fn test_login_success() {
    let (mut client, _pool) = setup().await;
    client
        .register(Request::new(RegisterRequest {
            email: "login@example.com".to_string(),
            password: "secret123".to_string(),
            display_name: "Login User".to_string(),
        }))
        .await
        .unwrap();
    let res = client
        .login(Request::new(LoginRequest {
            email: "login@example.com".to_string(),
            password: "secret123".to_string(),
        }))
        .await;
    assert!(res.is_ok());
    let login_resp = res.unwrap().into_inner();
    assert!(!login_resp.access_token.is_empty());
    assert!(!login_resp.organizations.is_empty());
}

#[tokio::test]
#[serial_test::serial]
async fn test_login_wrong_password() {
    let (mut client, _pool) = setup().await;
    client
        .register(Request::new(RegisterRequest {
            email: "wrong@example.com".to_string(),
            password: "correct123".to_string(),
            display_name: "Wrong PW".to_string(),
        }))
        .await
        .unwrap();
    let res = client
        .login(Request::new(LoginRequest {
            email: "wrong@example.com".to_string(),
            password: "incorrect".to_string(),
        }))
        .await;
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
#[serial_test::serial]
async fn test_get_profile() {
    let (mut client, _pool) = setup().await;
    client
        .register(Request::new(RegisterRequest {
            email: "profile@example.com".to_string(),
            password: "password123".to_string(),
            display_name: "Profile User".to_string(),
        }))
        .await
        .unwrap();

    let login = client
        .login(Request::new(LoginRequest {
            email: "profile@example.com".to_string(),
            password: "password123".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let res = client
        .get_profile(with_bearer(GetProfileRequest {}, &login.access_token))
        .await;
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().into_inner().user.unwrap().email,
        "profile@example.com"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_get_profile_missing_metadata() {
    let (mut client, _pool) = setup().await;
    let res = client.get_profile(Request::new(GetProfileRequest {})).await;
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
#[serial_test::serial]
async fn test_list_organizations() {
    let (mut client, _pool) = setup().await;
    client
        .register(Request::new(RegisterRequest {
            email: "orgs@example.com".to_string(),
            password: "password123".to_string(),
            display_name: "Org User".to_string(),
        }))
        .await
        .unwrap();

    let login = client
        .login(Request::new(LoginRequest {
            email: "orgs@example.com".to_string(),
            password: "password123".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let res = client
        .list_organizations(with_bearer(
            ListOrganizationsRequest {},
            &login.access_token,
        ))
        .await;
    assert!(res.is_ok());
    let orgs = res.unwrap().into_inner().organizations;
    assert_eq!(orgs.len(), 1);
    assert!(orgs[0].name.contains("Org User"));
}

#[tokio::test]
#[serial_test::serial]
async fn test_register_invalid_email() {
    let (mut client, _pool) = setup().await;
    let res = client
        .register(Request::new(RegisterRequest {
            email: "not-an-email".to_string(),
            password: "password123".to_string(),
            display_name: "Bad Email".to_string(),
        }))
        .await;

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
#[serial_test::serial]
async fn test_register_transaction_rolls_back_when_org_insert_fails() {
    let (mut client, pool) = setup().await;
    let too_long_name = "A".repeat(255);

    let res = client
        .register(Request::new(RegisterRequest {
            email: "rollback@example.com".to_string(),
            password: "password123".to_string(),
            display_name: too_long_name,
        }))
        .await;

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), tonic::Code::Internal);

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = ?")
        .bind("rollback@example.com")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(user_count, 0, "transaction must rollback user insert");
}

#[tokio::test]
#[serial_test::serial]
async fn test_logout_revokes_token() {
    let (mut client, _pool) = setup().await;

    client
        .register(Request::new(RegisterRequest {
            email: "logout@example.com".to_string(),
            password: "password123".to_string(),
            display_name: "Logout User".to_string(),
        }))
        .await
        .unwrap();

    let login = client
        .login(Request::new(LoginRequest {
            email: "logout@example.com".to_string(),
            password: "password123".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let token = login.access_token;

    client
        .logout(with_bearer(LogoutRequest {}, &token))
        .await
        .unwrap();

    let refresh = client
        .refresh_token(with_bearer(RefreshTokenRequest {}, &token))
        .await;
    assert!(refresh.is_err());
    assert_eq!(refresh.unwrap_err().code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
#[serial_test::serial]
async fn test_refresh_revokes_old_token() {
    let (mut client, _pool) = setup().await;

    client
        .register(Request::new(RegisterRequest {
            email: "refresh@example.com".to_string(),
            password: "password123".to_string(),
            display_name: "Refresh User".to_string(),
        }))
        .await
        .unwrap();

    let login = client
        .login(Request::new(LoginRequest {
            email: "refresh@example.com".to_string(),
            password: "password123".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let old_token = login.access_token;
    let refresh = client
        .refresh_token(with_bearer(RefreshTokenRequest {}, &old_token))
        .await
        .unwrap()
        .into_inner();

    assert!(!refresh.access_token.is_empty());

    let second = client
        .refresh_token(with_bearer(RefreshTokenRequest {}, &old_token))
        .await;
    assert!(second.is_err());
    assert_eq!(second.unwrap_err().code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
#[serial_test::serial]
async fn test_change_password_updates_credentials() {
    let (mut client, _pool) = setup().await;

    client
        .register(Request::new(RegisterRequest {
            email: "changepw@example.com".to_string(),
            password: "password123".to_string(),
            display_name: "Change Pw User".to_string(),
        }))
        .await
        .unwrap();

    let login = client
        .login(Request::new(LoginRequest {
            email: "changepw@example.com".to_string(),
            password: "password123".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    client
        .change_password(with_bearer(
            ChangePasswordRequest {
                current_password: "password123".to_string(),
                new_password: "newpassword123".to_string(),
            },
            &login.access_token,
        ))
        .await
        .unwrap();

    let old_login = client
        .login(Request::new(LoginRequest {
            email: "changepw@example.com".to_string(),
            password: "password123".to_string(),
        }))
        .await;
    assert!(old_login.is_err());
    assert_eq!(old_login.unwrap_err().code(), tonic::Code::Unauthenticated);

    let new_login = client
        .login(Request::new(LoginRequest {
            email: "changepw@example.com".to_string(),
            password: "newpassword123".to_string(),
        }))
        .await;
    assert!(new_login.is_ok());
}

#[tokio::test]
#[serial_test::serial]
async fn test_forgot_password_always_returns_success() {
    let (mut client, _pool) = setup().await;

    let known = client
        .forgot_password(Request::new(ForgotPasswordRequest {
            email: "known@example.com".to_string(),
        }))
        .await;
    assert!(known.is_ok());

    let unknown = client
        .forgot_password(Request::new(ForgotPasswordRequest {
            email: "unknown@example.com".to_string(),
        }))
        .await;
    assert!(unknown.is_ok());
}

#[tokio::test]
#[serial_test::serial]
async fn test_reset_password_with_invalid_token() {
    let (mut client, _pool) = setup().await;

    let res = client
        .reset_password(Request::new(ResetPasswordRequest {
            token: "invalid-token".to_string(),
            new_password: "newpassword123".to_string(),
        }))
        .await;

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
#[serial_test::serial]
async fn test_p1_invite_accept_and_list_org_members() {
    let (mut client, _pool) = setup().await;

    // Owner of target org
    let owner_email = "owner-p1@example.com";
    client
        .register(Request::new(RegisterRequest {
            email: owner_email.to_string(),
            password: "password123".to_string(),
            display_name: "Owner P1".to_string(),
        }))
        .await
        .unwrap();

    let owner_login = client
        .login(Request::new(LoginRequest {
            email: owner_email.to_string(),
            password: "password123".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    let owner_token = owner_login.access_token;
    let org_id = owner_login.organizations[0].id.clone();

    // Invitee account must exist
    let invitee_email = "invitee-p1@example.com";
    client
        .register(Request::new(RegisterRequest {
            email: invitee_email.to_string(),
            password: "password123".to_string(),
            display_name: "Invitee P1".to_string(),
        }))
        .await
        .unwrap();

    let invite_resp = client
        .invite_member(with_bearer(
            InviteMemberRequest {
                org_id: org_id.clone(),
                invitee_email: invitee_email.to_string(),
                org_role: OrgRole::OrMember as i32,
            },
            &owner_token,
        ))
        .await
        .unwrap()
        .into_inner();

    assert!(!invite_resp.invite_token.is_empty());

    client
        .accept_invitation(Request::new(AcceptInvitationRequest {
            token: invite_resp.invite_token,
        }))
        .await
        .unwrap();

    let members = client
        .list_org_members(with_bearer(
            ListOrgMembersRequest {
                org_id: org_id.clone(),
            },
            &owner_token,
        ))
        .await
        .unwrap()
        .into_inner()
        .members;

    assert!(members.iter().any(|m| m.email == invitee_email));
}

#[tokio::test]
#[serial_test::serial]
async fn test_p1_change_role_and_remove_member() {
    let (mut client, _pool) = setup().await;

    let owner_email = "owner-p1b@example.com";
    let member_email = "member-p1b@example.com";

    client
        .register(Request::new(RegisterRequest {
            email: owner_email.to_string(),
            password: "password123".to_string(),
            display_name: "Owner P1B".to_string(),
        }))
        .await
        .unwrap();

    client
        .register(Request::new(RegisterRequest {
            email: member_email.to_string(),
            password: "password123".to_string(),
            display_name: "Member P1B".to_string(),
        }))
        .await
        .unwrap();

    let owner_login = client
        .login(Request::new(LoginRequest {
            email: owner_email.to_string(),
            password: "password123".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    let owner_token = owner_login.access_token;
    let org_id = owner_login.organizations[0].id.clone();

    let invite = client
        .invite_member(with_bearer(
            InviteMemberRequest {
                org_id: org_id.clone(),
                invitee_email: member_email.to_string(),
                org_role: OrgRole::OrMember as i32,
            },
            &owner_token,
        ))
        .await
        .unwrap()
        .into_inner();

    client
        .accept_invitation(Request::new(AcceptInvitationRequest {
            token: invite.invite_token,
        }))
        .await
        .unwrap();

    let members_before = client
        .list_org_members(with_bearer(
            ListOrgMembersRequest {
                org_id: org_id.clone(),
            },
            &owner_token,
        ))
        .await
        .unwrap()
        .into_inner()
        .members;

    let target = members_before
        .iter()
        .find(|m| m.email == member_email)
        .unwrap();

    client
        .change_org_member_role(with_bearer(
            ChangeOrgMemberRoleRequest {
                org_id: org_id.clone(),
                user_id: target.user_id.clone(),
                org_role: OrgRole::OrAdmin as i32,
            },
            &owner_token,
        ))
        .await
        .unwrap();

    let members_after_role = client
        .list_org_members(with_bearer(
            ListOrgMembersRequest {
                org_id: org_id.clone(),
            },
            &owner_token,
        ))
        .await
        .unwrap()
        .into_inner()
        .members;

    let updated = members_after_role
        .iter()
        .find(|m| m.email == member_email)
        .unwrap();
    assert_eq!(updated.role, OrgRole::OrAdmin as i32);

    client
        .remove_org_member(with_bearer(
            RemoveOrgMemberRequest {
                org_id: org_id.clone(),
                user_id: updated.user_id.clone(),
            },
            &owner_token,
        ))
        .await
        .unwrap();

    let members_after_remove = client
        .list_org_members(with_bearer(ListOrgMembersRequest { org_id }, &owner_token))
        .await
        .unwrap()
        .into_inner()
        .members;

    assert!(!members_after_remove.iter().any(|m| m.email == member_email));
}

#[tokio::test]
#[serial_test::serial]
async fn test_p1_non_member_cannot_list_org_members() {
    let (mut client, _pool) = setup().await;

    client
        .register(Request::new(RegisterRequest {
            email: "owner-p1c@example.com".to_string(),
            password: "password123".to_string(),
            display_name: "Owner P1C".to_string(),
        }))
        .await
        .unwrap();

    client
        .register(Request::new(RegisterRequest {
            email: "outsider-p1c@example.com".to_string(),
            password: "password123".to_string(),
            display_name: "Outsider P1C".to_string(),
        }))
        .await
        .unwrap();

    let owner_login = client
        .login(Request::new(LoginRequest {
            email: "owner-p1c@example.com".to_string(),
            password: "password123".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    let org_id = owner_login.organizations[0].id.clone();

    let outsider_login = client
        .login(Request::new(LoginRequest {
            email: "outsider-p1c@example.com".to_string(),
            password: "password123".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let res = client
        .list_org_members(with_bearer(
            ListOrgMembersRequest { org_id },
            &outsider_login.access_token,
        ))
        .await;

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), tonic::Code::PermissionDenied);
}
