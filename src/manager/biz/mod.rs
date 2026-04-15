mod admin;
mod auth;
pub mod authz;
mod organization;
mod password;
mod profile;
mod token;

pub use token::Claims;

use tonic::Status;

use crate::manager::repository::IdentityRepository;

#[derive(Clone, Debug)]
pub enum NotificationEvent {
    PasswordReset {
        email: String,
    },
    OrgInvitation {
        email: String,
        org_id: String,
        invitation_id: String,
    },
}

pub struct IdentityBiz {
    repo: IdentityRepository,
    config: philand_configs::IdentityServiceConfig,
    notify_queue: Option<philand_queue::QueueSender<NotificationEvent>>,
}

impl IdentityBiz {
    pub fn new(
        repo: IdentityRepository,
        config: philand_configs::IdentityServiceConfig,
        notify_queue: Option<philand_queue::QueueSender<NotificationEvent>>,
    ) -> Self {
        Self {
            repo,
            config,
            notify_queue,
        }
    }

    fn map_internal_error(error: impl ToString) -> Status {
        Status::internal(error.to_string())
    }

    async fn enqueue_notification(&self, event: NotificationEvent) {
        if let Some(tx) = &self.notify_queue {
            if let Err(err) = philand_queue::enqueue(tx, event).await {
                tracing::warn!("notification queue enqueue failed: {err}");
            }
        }
    }

    /// Central authorization gate. Call this at the top of every biz method that
    /// requires elevated access. Returns `Status::permission_denied` (→ HTTP 403)
    /// if the caller does not hold the required permission.
    pub async fn require_permission(
        &self,
        caller_user_id: &str,
        permission: authz::Permission,
    ) -> Result<(), Status> {
        use crate::converters::{base_status_from_db, user_type_from_db};
        use crate::pb::common::base::BaseStatus;
        use crate::pb::shared::user::UserType;

        let caller = self
            .repo
            .find_user_by_id(caller_user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::unauthenticated("User not found"))?;

        if base_status_from_db(&caller.status) != BaseStatus::BsActive {
            return Err(Status::permission_denied("Account is disabled"));
        }

        match permission {
            authz::Permission::ManageAnyUser | authz::Permission::ManageAnyOrganization => {
                if user_type_from_db(&caller.user_type) != UserType::UtSuperAdmin {
                    return Err(Status::permission_denied("Super admin permission required"));
                }
            }
        }

        Ok(())
    }
}
