use tonic::Status;

use crate::pb::service::identity::GetProfileResponse;

use super::IdentityBiz;

impl IdentityBiz {
    pub async fn get_profile(&self, user_id: &str) -> Result<GetProfileResponse, Status> {
        let db_user = self
            .repo
            .find_user_by_id(user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("User not found"))?;

        Ok(GetProfileResponse {
            user: Some(db_user.into()),
        })
    }
}
