use tonic::Status;

use crate::manager::validate;
use crate::pb::service::identity::GetProfileResponse;
use crate::pb::service::identity::UpdateProfileResponse;

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

    pub async fn update_profile(
        &self,
        user_id: &str,
        display_name: Option<&str>,
        avatar: Option<&str>,
        bio: Option<&str>,
        timezone: Option<&str>,
        locale: Option<&str>,
    ) -> Result<UpdateProfileResponse, Status> {
        validate::update_profile_input(display_name, avatar, bio, timezone, locale)?;

        self.repo
            .update_user_profile(user_id, display_name, avatar, bio, timezone, locale)
            .await
            .map_err(Self::map_internal_error)?;

        let db_user = self
            .repo
            .find_user_by_id(user_id)
            .await
            .map_err(Self::map_internal_error)?
            .ok_or_else(|| Status::not_found("User not found"))?;

        Ok(UpdateProfileResponse {
            user: Some(db_user.into()),
        })
    }
}
