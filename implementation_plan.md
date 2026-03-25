# Implementation Plan: P0 (Auth Hardening) + P1 (Organization IAM)

## Overview

Expand the Identity service with 10 new REST endpoints (5 P0 + 5 P1) plus their corresponding gRPC RPCs, database tables, and business logic.

## P0: Auth Hardening Endpoints

| Endpoint | Auth | Description |
|---|---|---|
| `POST /logout` | Bearer | Blacklist the current JWT (token revocation via DB) |
| `POST /refresh` | Bearer | Issue a new JWT with fresh expiry |
| `POST /change-password` | Bearer | Change password (requires current + new) |
| `POST /forgot-password` | Public | Generate a reset token, log it (email dispatch deferred to Notification service) |
| `POST /reset-password` | Public | Validate reset token and set new password |

## P1: Organization IAM Endpoints

| Endpoint | Auth | Description |
|---|---|---|
| `GET /organizations/{org_id}/members` | Bearer (org member) | List all members of an organization |
| `POST /organizations/{org_id}/invitations` | Bearer (owner/admin) | Invite a user to the organization by email |
| `POST /invitations/{token}/accept` | Public | Accept an invitation using the invite token |
| `PATCH /organizations/{org_id}/members/{user_id}/role` | Bearer (owner) | Change a member's role |
| `DELETE /organizations/{org_id}/members/{user_id}` | Bearer (owner/admin, not self) | Remove a member from the organization |

---

## Database Changes

### New migration: `20260128000002_auth_hardening_and_invitations.sql`

#### Table: `revoked_tokens`
```sql
CREATE TABLE IF NOT EXISTS revoked_tokens (
    token_hash VARCHAR(64) PRIMARY KEY,   -- SHA-256 of the JWT
    user_id    VARCHAR(36) NOT NULL,
    expires_at TIMESTAMP NOT NULL,         -- TTL: auto-cleanup after JWT natural expiry
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
```

#### Table: `password_reset_tokens`
```sql
CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id         VARCHAR(36) PRIMARY KEY,
    user_id    VARCHAR(36) NOT NULL,
    token_hash VARCHAR(64) NOT NULL UNIQUE,  -- SHA-256 of the random token
    expires_at TIMESTAMP NOT NULL,
    used_at    TIMESTAMP NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
```

#### Table: `organization_invitations`
```sql
CREATE TABLE IF NOT EXISTS organization_invitations (
    id            VARCHAR(36) PRIMARY KEY,
    org_id        VARCHAR(36) NOT NULL,
    inviter_id    VARCHAR(36) NOT NULL,
    invitee_email VARCHAR(255) NOT NULL,
    org_role      VARCHAR(20) NOT NULL COMMENT 'admin | member',
    token_hash    VARCHAR(64) NOT NULL UNIQUE,  -- SHA-256 of the invite token
    status        VARCHAR(20) NOT NULL COMMENT 'pending | accepted | expired | revoked',
    expires_at    TIMESTAMP NOT NULL,
    created_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    FOREIGN KEY (org_id) REFERENCES organizations(id) ON DELETE CASCADE,
    FOREIGN KEY (inviter_id) REFERENCES users(id) ON DELETE CASCADE,
    UNIQUE KEY uk_org_email (org_id, invitee_email)
);
```

---

## New Proto Enums

### `InvitationStatus` (in `organization.proto`)
```protobuf
enum InvitationStatus {
    IS_NONE = 0;
    IS_PENDING = 1;
    IS_ACCEPTED = 2;
    IS_EXPIRED = 3;
    IS_REVOKED = 4;
}
```

No new `TokenPurpose` enum needed — the two token tables are structurally different (revoked_tokens vs password_reset_tokens), so the purpose is implicit in the table.

---

## New Proto RPCs (identity.proto)

```protobuf
// P0
rpc Logout(LogoutRequest) returns (LogoutResponse);
rpc RefreshToken(RefreshTokenRequest) returns (RefreshTokenResponse);
rpc ChangePassword(ChangePasswordRequest) returns (ChangePasswordResponse);
rpc ForgotPassword(ForgotPasswordRequest) returns (ForgotPasswordResponse);
rpc ResetPassword(ResetPasswordRequest) returns (ResetPasswordResponse);

// P1
rpc ListOrgMembers(ListOrgMembersRequest) returns (ListOrgMembersResponse);
rpc InviteMember(InviteMemberRequest) returns (InviteMemberResponse);
rpc AcceptInvitation(AcceptInvitationRequest) returns (AcceptInvitationResponse);
rpc ChangeOrgMemberRole(ChangeOrgMemberRoleRequest) returns (ChangeOrgMemberRoleResponse);
rpc RemoveOrgMember(RemoveOrgMemberRequest) returns (RemoveOrgMemberResponse);
```

---

## Files Modified or Created

### Modified
| File | Changes |
|---|---|
| `protobuf/shared/organization/organization.proto` | Add `InvitationStatus` enum |
| `protobuf/identity/identity.proto` | Add 10 new RPCs + request/response messages |
| `identity/src/converters/mod.rs` | Add `DbRevokedToken`, `DbPasswordResetToken`, `DbOrganizationInvitation`, `DbOrgMemberRow` structs; Add `invitation_status_to_db()`/`invitation_status_from_db()` helpers |
| `identity/src/manager/repository/mod.rs` | Add ~12 new queries for tokens, passwords, invitations, members |
| `identity/src/manager/validate/mod.rs` | Add validators for change-password, forgot-password, reset-password, invite-member, change-role |
| `identity/src/manager/biz/mod.rs` | Add `mod password;` and `mod invitation;` |
| `identity/src/manager/biz/auth.rs` | Add `logout()`, `refresh_token()` methods |
| `identity/src/manager/biz/token.rs` | Add `hash_token()` utility, token revocation check in `verify_jwt()` |
| `identity/src/handler/rest.rs` | Add 10 new endpoint handlers, DTOs, utoipa annotations, router routes |
| `identity/src/handler/grpc.rs` | Add 10 new RPC implementations |
| `identity/Cargo.toml` | Add `sha2` dependency for token hashing |

### Created
| File | Purpose |
|---|---|
| `identity/migrations/20260128000002_auth_hardening_and_invitations.sql` | New tables |
| `identity/src/manager/biz/password.rs` | `change_password()`, `forgot_password()`, `reset_password()` |
| `identity/src/manager/biz/invitation.rs` | `list_org_members()`, `invite_member()`, `accept_invitation()`, `change_member_role()`, `remove_member()` |

---

## Implementation Order

1. **Proto contracts** — Update `.proto` files first (enum + RPCs + messages)
2. **DB migration** — Create the new migration file
3. **Converters** — Add new DB model structs + `InvitationStatus` enum helpers
4. **Repository** — Add all new DB queries
5. **Validators** — Add input validation for new endpoints
6. **Biz layer** — Implement P0 business logic (password.rs, auth.rs additions)
7. **Biz layer** — Implement P1 business logic (invitation.rs)
8. **REST handlers** — Add all 10 new HTTP endpoints + DTOs + utoipa
9. **gRPC handlers** — Add all 10 new RPC implementations
10. **Build & verify** — `cargo build`, `cargo clippy`

---

## Security Design Decisions

- **Token revocation**: On logout, SHA-256 hash of the JWT is stored in `revoked_tokens`. `verify_jwt()` checks this table. Expired tokens are naturally prunable.
- **Password reset tokens**: Random 32-byte hex token, stored as SHA-256 hash. 1-hour expiry. Single use (`used_at` set on consume).
- **Invitation tokens**: Random 32-byte hex token, SHA-256 hashed. 7-day expiry. Unique constraint on (org_id, invitee_email) prevents duplicate invites.
- **Role-based access**: `invite_member` and `remove_member` require owner or admin. `change_role` requires owner only. Verified by querying the caller's role from `organization_members`.

---

## Downstream Impact

- **Gateway**: No changes needed. All new endpoints share the `/api/identity/*` prefix already routed.
- **OpenAPI**: Automatically updated — new endpoints get utoipa annotations, gateway fetches live spec.
- **Notification service**: `forgot_password` and `invite_member` will eventually emit events for email dispatch, but for now they just log the tokens. This is a known deferral.

---

**Status: IMPLEMENTED (P0 + P1) AND VALIDATED IN IDENTITY TEST SUITE**
