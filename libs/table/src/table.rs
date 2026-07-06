pub const USERS: &str = "philandz.users";
pub const ORGANIZATIONS: &str = "philandz.organizations";
pub const ORGANIZATION_MEMBERS: &str = "philandz.organization_members";
pub const REVOKED_TOKENS: &str = "philandz.revoked_tokens";
pub const PASSWORD_RESET_TOKENS: &str = "philandz.password_reset_tokens";
pub const ORGANIZATION_INVITATIONS: &str = "philandz.organization_invitations";

// Platform-wide settings — secrets live AES-GCM encrypted in
// `platform_settings`, while non-secret mail config (from-address, etc.)
// lives unmasked in `platform_settings_public` so the rest of the system
// can read it without holding the master key.
pub const PLATFORM_SETTINGS: &str = "philandz.platform_settings";
pub const PLATFORM_SETTINGS_PUBLIC: &str = "philandz.platform_settings_public";

// One-time codes required to confirm a logged-in password change.
pub const PASSWORD_CHANGE_OTPS: &str = "philandz.password_change_otps";
