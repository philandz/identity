# Go to Rust Migration Roadmap

## Goals

1. Full package migration from `pkcore` and `biz` into Rust workspace crates.
2. Priority-first delivery for:
   - `configs`
   - `logging`
   - `crypto`
   - `oauth2` (Google first)
   - `storage` (MySQL + S3 + presigned URLs)
3. Unit test coverage target >80% (enforced in libs CI only).

## Phase Plan

### Phase 1 (in progress)

- Scaffold Rust workspace and priority crates.
- Implement initial APIs and baseline tests.
- Validate compile and test execution.

### Phase 2

- Expand priority crates to full parity:
  - oauth2 refresh/token edge cases
  - storage retry/timeouts
  - config validation + defaults
  - structured logging filters

### Phase 3

- Port remaining utility domains (`http`, `queue`, `notify`, `socket`, ...).

### Phase 4

- Adopt crates in service repos (`identity`, `gateway`, future services).
- Remove duplicated code in service-local modules.

## Testing and Coverage

- Use `cargo test` for crate tests.
- Use `cargo llvm-cov` in libs CI.
- Gate merge if total coverage <80%.

## Required Runtime Inputs

- Shared DB endpoint from env: `DATABASE_URL` points to `philand` DB.
- OAuth2 Google secrets from env / k3s secrets.
- S3 custom endpoint and creds from env / k3s secrets.
