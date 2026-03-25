## Identity Refactor Walkthrough

### Phase 2A-2D: Service Structure + Business Logic

- Refactored the gRPC handler into modular files:
  - `src/handler/grpc.rs`
  - `src/handler/metadata.rs`
  - `src/handler/mod.rs` now re-exports `IdentityHandler`
- Refactored business logic into focused modules:
  - `src/manager/biz/auth.rs`
  - `src/manager/biz/profile.rs`
  - `src/manager/biz/organization.rs`
  - `src/manager/biz/token.rs`
  - `src/manager/biz/mod.rs` keeps shared mappings and construction
- Added a validation layer:
  - `src/manager/validate/mod.rs`
  - register/login input validation now returns `InvalidArgument` for bad input
- Added transactional registration in repository:
  - `src/manager/repository/mod.rs` now has `create_user_with_default_organization`
  - user + organization + membership are written inside one SQL transaction
  - added unique-constraint detection helper for duplicate email handling
- Expanded integration tests in `tests/identity_service_test.rs`:
  - invalid email path
  - transaction rollback behavior when organization insert fails
  - updated test passwords to satisfy stronger validation

### Phase 2D-fix: JWT Claims (email + org_id)

- Updated `src/manager/biz/token.rs`:
  - JWT `Claims` now contains `{ sub, email, org_id, exp }` per spec (`jwt-and-org.md`)
  - Removed `user_type` from claims (not required by spec)
  - `issue_jwt()` now accepts `org_id` parameter
  - `Claims` struct fields are `pub` for downstream verification/extraction
- Updated `src/manager/biz/auth.rs`:
  - Login flow fetches org summaries before issuing JWT
  - First organization ID is used as default `org_id` in token
  - Empty string if user has no organizations

### Phase 2D-fix: Consul Registration + KV Config

- Updated `src/config/mod.rs`:
  - Added `consul_addr` and `service_name` fields to `AppConfig`
  - Added `register_consul()` — registers service with Consul HTTP API (best-effort, warns on failure)
  - Added `read_consul_kv()` — reads `config/identity/*` keys from Consul KV store
  - Both methods are resilient: service starts even if Consul is unavailable
- Updated `src/main.rs`:
  - Calls `config.register_consul()` and `config.read_consul_kv()` after migrations
- Added `reqwest` and `base64` dependencies to `Cargo.toml`

### Phase 2D-fix: OpenAPI (utoipa) Exposure

- Updated `src/main.rs`:
  - Added `#[derive(OpenApi)]` struct `ApiDoc` with identity service info
  - HTTP server now serves `/api-docs/openapi.json` (JSON spec) and `/docs` (Swagger UI)
  - `health_check` handler annotated with `#[utoipa::path]`
- Added `utoipa` and `utoipa-swagger-ui` dependencies to `Cargo.toml`

### Phase 2E: Gateway Cut-Over

Changes in `gateway/`:

- **Service-aware routing** (`src/proxy/mod.rs`):
  - `/api/identity/*` routes to the identity service HTTP port (strips `/identity` prefix)
  - All other `/api/*` routes continue to the monolith (Strangler Fig Pattern)
  - Shared `forward_request()` function eliminates duplication

- **X-Request-Id** (`src/main.rs`):
  - Added `SetRequestIdLayer` + `PropagateRequestIdLayer` from `tower-http`
  - Every incoming request gets a UUID `x-request-id` header if not present
  - Header is propagated to upstream services and back to the client

- **Swagger aggregation** (`src/swagger/mod.rs`):
  - Swagger UI now lists two specs: "Gateway" and "Identity"
  - Identity spec is fetched live from the identity service at `/api-docs/openapi.json`
  - Falls back to a stub spec if identity service is unreachable

- **AppState** (`src/lib.rs`):
  - Added `identity_url` field for identity service base URL
  - `IDENTITY_URL` env var (defaults to `http://127.0.0.1:3001`)

- **Dependencies** (`Cargo.toml`):
  - Upgraded `tower-http` to 0.6 with `request-id` + `util` features
  - Added `uuid` for request ID generation

- All 8 existing gateway tests updated and passing

### Validation

| Check | Result |
|-------|--------|
| `cargo clippy -- -D warnings` (identity) | Pass, zero warnings |
| `cargo clippy` (gateway) | Pass, zero warnings |
| `cargo test` (gateway) | 8/8 tests pass |
| `cargo test` (identity) | Pass (17/17 tests) |
| `cargo run` (identity) | Pass (local) |

### Phase 2E+: P1 Organization IAM completion

- Added P1 gRPC RPCs and REST endpoints:
  - `ListOrgMembers`, `InviteMember`, `AcceptInvitation`, `ChangeOrgMemberRole`, `RemoveOrgMember`
- Added DB migration for invitation workflow:
  - `migrations/20260128000003_organization_invitations.sql`
- Added role-based access rules in biz layer:
  - owner/admin invite and remove, owner-only role changes, non-member denied
- Added P1 integration tests for end-to-end IAM flow in `tests/identity_service_test.rs`:
  - invite + accept + list
  - change role + remove member
  - non-member authorization rejection

### To complete runtime validation

1. Start OrbStack / Docker daemon
2. `docker compose -f philand/docker-compose.dev.yml up -d database`
3. `cd identity && cargo test`
4. `cd identity && cargo run`
5. `curl http://127.0.0.1:3001/health`
6. `curl http://127.0.0.1:3001/api-docs/openapi.json`
7. `cd gateway && cargo run` (with `IDENTITY_URL=http://127.0.0.1:3001`)
8. `curl http://127.0.0.1:3000/api/identity/health`
9. Open `http://127.0.0.1:3000/docs` — verify both Gateway and Identity specs appear
