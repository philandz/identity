use tonic::Status;

use chrono::{Duration, Utc};

use crate::manager::repository::UpsertOrganizationInvitationParams;
use crate::manager::validate;
use crate::pb::service::identity::{
    AcceptInvitationResponse, ChangeOrgMemberRoleResponse, InviteMemberResponse,
    ListOrgMembersResponse, ListOrganizationsResponse, OrgMemberView, OrganizationInvitation,
    OrganizationSummary, RemoveOrgMemberResponse,
};
use crate::pb::shared::organization::{InvitationStatus, OrgRole};

use super::token::hash_token;
use super::IdentityBiz;

impl IdentityBiz {
    pub async fn list_organizations(
        &self,
        user_id: &str,
    ) -> Result<ListOrganizationsResponse, Status> {
        use crate::converters::org_role_from_db;
        let organizations = self
            .repo
            .find_user_organizations(user_id)
            .await
            .map_err(Self::map_internal_error)?
            .into_iter()
            .map(|o| OrganizationSummary {
                id: o.id,
                name: o.name,
                role: org_role_from_db(&o.org_role) as i32,
            })
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
            "Organization invitation created for {} in org {} (id={})",
            normalized_email,
            org_id,
            invitation_id
        );

        self.enqueue_notification(super::NotificationEvent::OrgInvitation {
            email: normalized_email.clone(),
            org_id: org_id.to_string(),
            invitation_id: invitation_id.clone(),
        })
        .await;

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

    pub async fn transfer_org_ownership(
        &self,
        caller_user_id: &str,
        org_id: &str,
        new_owner_id: &str,
    ) -> Result<(), Status> {
        if caller_user_id == new_owner_id {
            return Err(Status::invalid_argument(
                "You are already the owner of this organization",
            ));
        }

        let caller_role = self
            .repo
            .find_org_member_role(org_id, caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::permission_denied("Caller is not an active org member"))?;
        if caller_role != OrgRole::OrOwner {
            return Err(Status::permission_denied(
                "Only the current owner can transfer ownership",
            ));
        }

        let target_role = self
            .repo
            .find_org_member_role(org_id, new_owner_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("Target member not found in this organization"))?;
        if target_role == OrgRole::OrOwner {
            return Err(Status::invalid_argument("Target is already the owner"));
        }

        self.repo
            .transfer_org_ownership(org_id, caller_user_id, new_owner_id)
            .await
            .map_err(Self::map_internal_error)?;

        Ok(())
    }

    pub async fn rename_organization(
        &self,
        caller_user_id: &str,
        org_id: &str,
        new_name: &str,
    ) -> Result<(), Status> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() || trimmed.len() > 100 {
            return Err(Status::invalid_argument(
                "Organization name must be between 1 and 100 characters",
            ));
        }

        let caller_role = self
            .repo
            .find_org_member_role(org_id, caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::permission_denied("Caller is not an active org member"))?;
        if caller_role != OrgRole::OrOwner {
            return Err(Status::permission_denied(
                "Only owner can rename the organization",
            ));
        }

        let updated = self
            .repo
            .rename_organization(org_id, trimmed)
            .await
            .map_err(Self::map_internal_error)?;
        if updated == 0 {
            return Err(Status::not_found("Organization not found"));
        }

        Ok(())
    }

    pub async fn list_org_invitations(
        &self,
        caller_user_id: &str,
        org_id: &str,
    ) -> Result<Vec<crate::manager::repository::OrganizationInvitationRow>, Status> {
        let caller_role = self
            .repo
            .find_org_member_role(org_id, caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::permission_denied("Caller is not an active org member"))?;
        if !matches!(caller_role, OrgRole::OrOwner | OrgRole::OrAdmin) {
            return Err(Status::permission_denied(
                "Only owner/admin can view invitations",
            ));
        }

        self.repo
            .find_pending_invitations_by_org(org_id)
            .await
            .map_err(Self::map_internal_error)
    }

    pub async fn revoke_invitation(
        &self,
        caller_user_id: &str,
        org_id: &str,
        invitation_id: &str,
    ) -> Result<(), Status> {
        let caller_role = self
            .repo
            .find_org_member_role(org_id, caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::permission_denied("Caller is not an active org member"))?;
        if !matches!(caller_role, OrgRole::OrOwner | OrgRole::OrAdmin) {
            return Err(Status::permission_denied(
                "Only owner/admin can revoke invitations",
            ));
        }

        let revoked = self
            .repo
            .revoke_invitation(org_id, invitation_id)
            .await
            .map_err(Self::map_internal_error)?;
        if revoked == 0 {
            return Err(Status::not_found(
                "Invitation not found or already processed",
            ));
        }

        Ok(())
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
    philand_random::random_string(64)
}
