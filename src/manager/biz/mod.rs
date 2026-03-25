mod auth;
mod organization;
mod password;
mod profile;
mod token;

pub use token::Claims;

use tonic::Status;

use crate::config::AppConfig;
use crate::manager::repository::IdentityRepository;

pub struct IdentityBiz {
    repo: IdentityRepository,
    config: AppConfig,
}

impl IdentityBiz {
    pub fn new(repo: IdentityRepository, config: AppConfig) -> Self {
        Self { repo, config }
    }

    fn map_internal_error(error: impl ToString) -> Status {
        Status::internal(error.to_string())
    }
}
