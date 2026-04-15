#![allow(clippy::result_large_err)]

use tonic::Status;

use crate::pb::common::base::BaseStatus;
use crate::pb::shared::organization::OrgRole;
use crate::pb::shared::user::UserType;

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

pub fn update_profile_input(
    display_name: Option<&str>,
    avatar: Option<&str>,
    bio: Option<&str>,
    timezone: Option<&str>,
    locale: Option<&str>,
) -> Result<(), Status> {
    if display_name.is_none()
        && avatar.is_none()
        && bio.is_none()
        && timezone.is_none()
        && locale.is_none()
    {
        return Err(Status::invalid_argument(
            "At least one profile field must be provided",
        ));
    }

    if let Some(v) = display_name {
        display_name_value(v)?;
    }

    if let Some(v) = avatar {
        avatar_url_value(v)?;
    }

    if let Some(v) = bio {
        if v.len() > 1000 {
            return Err(Status::invalid_argument("Bio is too long"));
        }
    }

    if let Some(v) = timezone {
        if v.trim().is_empty() || v.len() > 50 {
            return Err(Status::invalid_argument("Timezone is invalid"));
        }
    }

    if let Some(v) = locale {
        if v.trim().is_empty() || v.len() > 10 {
            return Err(Status::invalid_argument("Locale is invalid"));
        }
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

pub fn admin_get_user_input(user_id: &str) -> Result<(), Status> {
    if user_id.trim().is_empty() {
        return Err(Status::invalid_argument("user_id must not be empty"));
    }
    Ok(())
}

/// Generic non-empty ID check used by admin delete/get operations.
pub fn non_empty_id(field: &str, value: &str) -> Result<(), Status> {
    if value.trim().is_empty() {
        return Err(Status::invalid_argument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

pub fn admin_create_organization_input(name: &str, owner_user_id: &str) -> Result<(), Status> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument(
            "Organization name must not be empty",
        ));
    }
    if trimmed.len() > 255 {
        return Err(Status::invalid_argument("Organization name is too long"));
    }
    if owner_user_id.trim().is_empty() {
        return Err(Status::invalid_argument("owner_user_id must not be empty"));
    }
    Ok(())
}

pub fn admin_update_user_input(
    user_id: &str,
    display_name: Option<&str>,
    user_type: Option<i32>,
    status: Option<i32>,
) -> Result<(), Status> {
    admin_get_user_input(user_id)?;

    if display_name.is_none() && user_type.is_none() && status.is_none() {
        return Err(Status::invalid_argument(
            "At least one user field must be provided",
        ));
    }

    if let Some(v) = display_name {
        display_name_value(v)?;
    }

    if let Some(v) = user_type {
        let parsed = UserType::try_from(v).unwrap_or(UserType::UtNone);
        if !matches!(parsed, UserType::UtNormal | UserType::UtSuperAdmin) {
            return Err(Status::invalid_argument(
                "user_type must be normal or super_admin",
            ));
        }
    }

    if let Some(v) = status {
        let parsed = BaseStatus::try_from(v).unwrap_or(BaseStatus::BsUnknown);
        if !matches!(parsed, BaseStatus::BsActive | BaseStatus::BsDisabled) {
            return Err(Status::invalid_argument(
                "status must be active or disabled",
            ));
        }
    }

    Ok(())
}

pub fn admin_get_organization_input(org_id: &str) -> Result<(), Status> {
    if org_id.trim().is_empty() {
        return Err(Status::invalid_argument("org_id must not be empty"));
    }
    Ok(())
}

pub fn admin_update_organization_input(
    org_id: &str,
    name: Option<&str>,
    status: Option<i32>,
) -> Result<(), Status> {
    admin_get_organization_input(org_id)?;

    if name.is_none() && status.is_none() {
        return Err(Status::invalid_argument(
            "At least one organization field must be provided",
        ));
    }

    if let Some(v) = name {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err(Status::invalid_argument(
                "Organization name must not be empty",
            ));
        }
        if trimmed.len() > 255 {
            return Err(Status::invalid_argument("Organization name is too long"));
        }
    }

    if let Some(v) = status {
        let parsed = BaseStatus::try_from(v).unwrap_or(BaseStatus::BsUnknown);
        if !matches!(parsed, BaseStatus::BsActive | BaseStatus::BsDisabled) {
            return Err(Status::invalid_argument(
                "status must be active or disabled",
            ));
        }
    }

    Ok(())
}

fn email_value(email: &str) -> Result<(), Status> {
    let trimmed = email.trim();
    if philand_validator::non_empty("email", trimmed).is_err() {
        return Err(Status::invalid_argument("Email must not be empty"));
    }

    if philand_validator::email(trimmed).is_err() {
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
    if philand_validator::non_empty("display_name", trimmed).is_err() {
        return Err(Status::invalid_argument("Display name must not be empty"));
    }

    if trimmed.len() > 255 {
        return Err(Status::invalid_argument("Display name is too long"));
    }

    Ok(())
}

fn avatar_url_value(avatar: &str) -> Result<(), Status> {
    let trimmed = avatar.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    if philand_validator::url(trimmed).is_err() {
        return Err(Status::invalid_argument(
            "Avatar must be a valid http/https URL",
        ));
    }

    Ok(())
}
