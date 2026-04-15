/// Fine-grained permission tokens used by [`super::biz::IdentityBiz::require_permission`].
///
/// Every biz method that needs access control calls `require_permission` with the
/// appropriate variant. The check is centralised here — no scattered `ensure_*` helpers.
#[derive(Debug)]
pub enum Permission {
    /// Read/write any user record — super_admin only.
    ManageAnyUser,
    /// Read/write any organization record — super_admin only.
    ManageAnyOrganization,
}
