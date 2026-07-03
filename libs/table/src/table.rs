pub const USERS: &str = "philand.users";
pub const ORGANIZATIONS: &str = "philand.organizations";
pub const ORGANIZATION_MEMBERS: &str = "philand.organization_members";
pub const REVOKED_TOKENS: &str = "philand.revoked_tokens";
pub const PASSWORD_RESET_TOKENS: &str = "philand.password_reset_tokens";
pub const ORGANIZATION_INVITATIONS: &str = "philand.organization_invitations";

// Platform-wide settings — secrets live AES-GCM encrypted in
// `platform_settings`, while non-secret mail config (from-address, etc.)
// lives unmasked in `platform_settings_public` so the rest of the system
// can read it without holding the master key.
pub const PLATFORM_SETTINGS: &str = "philand.platform_settings";
pub const PLATFORM_SETTINGS_PUBLIC: &str = "philand.platform_settings_public";

// One-time codes required to confirm a logged-in password change.
pub const PASSWORD_CHANGE_OTPS: &str = "philand.password_change_otps";
