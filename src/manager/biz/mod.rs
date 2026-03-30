mod auth;
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
        token: String,
    },
    OrgInvitation {
        email: String,
        org_id: String,
        token: String,
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
}
