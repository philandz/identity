#![allow(clippy::result_large_err)]

use tonic::Status;

use crate::pb::shared::organization::OrgRole;

const MIN_PASSWORD_LEN: usize = 8;
const MAX_PASSWORD_LEN: usize = 72;

pub fn register_input(email: &str, password: &str, display_name: &str) -> Result<(), Status> {
    email_value(email)?;
    password_value(password)?;
    display_name_value(display_name)?;
    Ok(())
}

pub fn login_input(email: &str, password: &str) -> Result<(), Status> {
    email_value(email)?;
    if password.trim().is_empty() {
        return Err(Status::invalid_argument("Password must not be empty"));
    }
    Ok(())
}

pub fn change_password_input(current_password: &str, new_password: &str) -> Result<(), Status> {
    if current_password.trim().is_empty() {
        return Err(Status::invalid_argument(
            "Current password must not be empty",
        ));
    }
    password_value(new_password)?;
    if current_password == new_password {
        return Err(Status::invalid_argument(
            "New password must differ from current password",
        ));
    }
    Ok(())
}

pub fn forgot_password_input(email: &str) -> Result<(), Status> {
    email_value(email)
}

pub fn reset_password_input(token: &str, new_password: &str) -> Result<(), Status> {
    if token.trim().is_empty() {
        return Err(Status::invalid_argument("Reset token must not be empty"));
    }
    password_value(new_password)
}

pub fn list_org_members_input(org_id: &str) -> Result<(), Status> {
    if org_id.trim().is_empty() {
        return Err(Status::invalid_argument("org_id must not be empty"));
    }
    Ok(())
}

pub fn invite_member_input(org_id: &str, invitee_email: &str, org_role: i32) -> Result<(), Status> {
    list_org_members_input(org_id)?;
    email_value(invitee_email)?;

    let role = OrgRole::try_from(org_role).unwrap_or(OrgRole::OrNone);
    if !matches!(role, OrgRole::OrAdmin | OrgRole::OrMember) {
        return Err(Status::invalid_argument("org_role must be admin or member"));
    }

    Ok(())
}

pub fn accept_invitation_input(token: &str) -> Result<(), Status> {
    if token.trim().is_empty() {
        return Err(Status::invalid_argument("token must not be empty"));
    }
    Ok(())
}

pub fn change_org_member_role_input(
    org_id: &str,
    user_id: &str,
    org_role: i32,
) -> Result<(), Status> {
    list_org_members_input(org_id)?;
    if user_id.trim().is_empty() {
        return Err(Status::invalid_argument("user_id must not be empty"));
    }

    let role = OrgRole::try_from(org_role).unwrap_or(OrgRole::OrNone);
    if !matches!(role, OrgRole::OrAdmin | OrgRole::OrMember) {
        return Err(Status::invalid_argument("org_role must be admin or member"));
    }

    Ok(())
}

pub fn remove_org_member_input(org_id: &str, user_id: &str) -> Result<(), Status> {
    list_org_members_input(org_id)?;
    if user_id.trim().is_empty() {
        return Err(Status::invalid_argument("user_id must not be empty"));
    }
    Ok(())
}

fn email_value(email: &str) -> Result<(), Status> {
    let trimmed = email.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument("Email must not be empty"));
    }

    let has_at = trimmed.contains('@');
    let has_domain = trimmed
        .split('@')
        .nth(1)
        .is_some_and(|part| part.contains('.'));
    if !has_at || !has_domain {
        return Err(Status::invalid_argument("Email format is invalid"));
    }

    Ok(())
}

fn password_value(password: &str) -> Result<(), Status> {
    let len = password.len();
    if len < MIN_PASSWORD_LEN {
        return Err(Status::invalid_argument(
            "Password must be at least 8 characters",
        ));
    }

    if len > MAX_PASSWORD_LEN {
        return Err(Status::invalid_argument(
            "Password must be at most 72 characters",
        ));
    }

    Ok(())
}

fn display_name_value(display_name: &str) -> Result<(), Status> {
    let trimmed = display_name.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument("Display name must not be empty"));
    }

    if trimmed.len() > 255 {
        return Err(Status::invalid_argument("Display name is too long"));
    }

    Ok(())
}
