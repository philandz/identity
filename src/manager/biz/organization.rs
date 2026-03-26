use tonic::Status;

use chrono::{Duration, Utc};

use crate::manager::repository::UpsertOrganizationInvitationParams;
use crate::manager::validate;
use crate::pb::service::identity::{
    AcceptInvitationResponse, ChangeOrgMemberRoleResponse, InviteMemberResponse,
    ListOrgMembersResponse, ListOrganizationsResponse, OrgMemberView, OrganizationInvitation,
    RemoveOrgMemberResponse,
};
use crate::pb::shared::organization::{InvitationStatus, OrgRole};

use super::token::hash_token;
use super::IdentityBiz;

impl IdentityBiz {
    pub async fn list_organizations(
        &self,
        user_id: &str,
    ) -> Result<ListOrganizationsResponse, Status> {
        let organizations = self
            .repo
            .find_user_organizations(user_id)
            .await
            .map_err(Self::map_internal_error)?
            .into_iter()
            .map(Into::into)
            .collect();

        Ok(ListOrganizationsResponse { organizations })
    }

    pub async fn list_org_members(
        &self,
        caller_user_id: &str,
        org_id: &str,
    ) -> Result<ListOrgMembersResponse, Status> {
        validate::list_org_members_input(org_id)?;

        let caller_role = self
            .repo
            .find_org_member_role(org_id, caller_user_id)
            .await
            .map_err(Self::map_internal_error)?;
        if caller_role.is_none() {
            return Err(Status::permission_denied(
                "Caller is not a member of this organization",
            ));
        }

        let members = self
            .repo
            .list_org_members(org_id)
            .await
            .map_err(Self::map_internal_error)?
            .into_iter()
            .map(|m| OrgMemberView {
                user_id: m.user_id,
                email: m.email,
                display_name: m.display_name,
                role: m.role as i32,
                status: m.status as i32,
                joined_at: m.joined_at,
            })
            .collect();

        Ok(ListOrgMembersResponse { members })
    }

    pub async fn invite_member(
        &self,
        caller_user_id: &str,
        org_id: &str,
        invitee_email: &str,
        org_role: i32,
    ) -> Result<InviteMemberResponse, Status> {
        validate::invite_member_input(org_id, invitee_email, org_role)?;

        let caller_role = self
            .repo
            .find_org_member_role(org_id, caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::permission_denied("Caller is not an active org member"))?;

        if !matches!(caller_role, OrgRole::OrOwner | OrgRole::OrAdmin) {
            return Err(Status::permission_denied(
                "Only owner/admin can invite members",
            ));
        }

        let normalized_email = invitee_email.trim().to_lowercase();

        let already_member = self
            .repo
            .find_user_by_email_active_member_of_org(org_id, &normalized_email)
            .await
            .map_err(Self::map_internal_error)?;
        if already_member {
            return Err(Status::already_exists(
                "User is already a member of this organization",
            ));
        }

        let role = OrgRole::try_from(org_role).unwrap_or(OrgRole::OrNone);
        let invitation_id = uuid::Uuid::new_v4().to_string();
        let raw_token = generate_random_token();
        let token_hash = hash_token(&raw_token);
        let now = Utc::now();
        let expires_at = now + Duration::days(7);

        self.repo
            .upsert_organization_invitation(UpsertOrganizationInvitationParams {
                id: &invitation_id,
                org_id,
                inviter_id: caller_user_id,
                invitee_email: &normalized_email,
                org_role: role,
                token_hash: &token_hash,
                status: InvitationStatus::IsPending,
                expires_at,
            })
            .await
            .map_err(Self::map_internal_error)?;

        tracing::info!(
            "Organization invitation token for {} in org {}: {}",
            normalized_email,
            org_id,
            raw_token
        );

        Ok(InviteMemberResponse {
            invitation: Some(OrganizationInvitation {
                id: invitation_id,
                org_id: org_id.to_string(),
                inviter_id: caller_user_id.to_string(),
                invitee_email: normalized_email,
                org_role: role as i32,
                status: InvitationStatus::IsPending as i32,
                expires_at: expires_at.timestamp(),
                created_at: now.timestamp(),
            }),
            invite_token: raw_token,
        })
    }

    pub async fn accept_invitation(&self, token: &str) -> Result<AcceptInvitationResponse, Status> {
        validate::accept_invitation_input(token)?;

        let token_hash = hash_token(token.trim());
        let invitation = self
            .repo
            .find_valid_invitation_by_token(&token_hash)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::invalid_argument("Invalid or expired invitation token"))?;

        let user = self
            .repo
            .find_user_by_email(&invitation.invitee_email)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| {
                Status::failed_precondition(
                    "Invited email has no account yet; register first before accepting",
                )
            })?;

        self.repo
            .accept_invitation_for_user(
                &invitation.id,
                &invitation.org_id,
                &user.id,
                invitation.org_role,
            )
            .await
            .map_err(Self::map_internal_error)?;

        Ok(AcceptInvitationResponse {
            org_id: invitation.org_id,
            role: match invitation.org_role {
                OrgRole::OrOwner => "owner",
                OrgRole::OrAdmin => "admin",
                OrgRole::OrMember => "member",
                OrgRole::OrNone => "none",
            }
            .to_string(),
        })
    }

    pub async fn change_org_member_role(
        &self,
        caller_user_id: &str,
        org_id: &str,
        target_user_id: &str,
        new_role: i32,
    ) -> Result<ChangeOrgMemberRoleResponse, Status> {
        validate::change_org_member_role_input(org_id, target_user_id, new_role)?;

        let caller_role = self
            .repo
            .find_org_member_role(org_id, caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::permission_denied("Caller is not an active org member"))?;
        if caller_role != OrgRole::OrOwner {
            return Err(Status::permission_denied("Only owner can change roles"));
        }

        if caller_user_id == target_user_id {
            return Err(Status::invalid_argument("Owner cannot change own role"));
        }

        let target_exists = self
            .repo
            .find_org_member_role(org_id, target_user_id)
            .await
            .map_err(Self::map_internal_error)?;
        if target_exists.is_none() {
            return Err(Status::not_found("Target member not found"));
        }

        let role = OrgRole::try_from(new_role).unwrap_or(OrgRole::OrNone);
        let updated = self
            .repo
            .update_org_member_role(org_id, target_user_id, role)
            .await
            .map_err(Self::map_internal_error)?;

        if updated == 0 {
            return Err(Status::not_found("Target member not found"));
        }

        Ok(ChangeOrgMemberRoleResponse {})
    }

    pub async fn remove_org_member(
        &self,
        caller_user_id: &str,
        org_id: &str,
        target_user_id: &str,
    ) -> Result<RemoveOrgMemberResponse, Status> {
        validate::remove_org_member_input(org_id, target_user_id)?;

        if caller_user_id == target_user_id {
            return Err(Status::invalid_argument("Cannot remove yourself"));
        }

        let caller_role = self
            .repo
            .find_org_member_role(org_id, caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::permission_denied("Caller is not an active org member"))?;
        if !matches!(caller_role, OrgRole::OrOwner | OrgRole::OrAdmin) {
            return Err(Status::permission_denied(
                "Only owner/admin can remove members",
            ));
        }

        let target_role = self
            .repo
            .find_org_member_role(org_id, target_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("Target member not found"))?;

        if target_role == OrgRole::OrOwner {
            return Err(Status::permission_denied(
                "Cannot remove organization owner",
            ));
        }
        if caller_role == OrgRole::OrAdmin && target_role == OrgRole::OrAdmin {
            return Err(Status::permission_denied(
                "Admin cannot remove another admin",
            ));
        }

        let deleted = self
            .repo
            .remove_org_member(org_id, target_user_id)
            .await
            .map_err(Self::map_internal_error)?;
        if deleted == 0 {
            return Err(Status::not_found("Target member not found"));
        }

        Ok(RemoveOrgMemberResponse {})
    }
}

fn generate_random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
