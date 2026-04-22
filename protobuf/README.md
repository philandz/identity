# protobuf

Shared protobuf contracts for Philand services.

## Layout Policy

- Shared contracts go under `protobuf/shared/<domain>/`.
- Service contracts go under `protobuf/<service>/` (flat service directory).
- Do not use `protobuf/service/<service>/`.

## Current Key Files

- `protobuf/identity/identity.proto`
- `protobuf/shared/user/user.proto`
- `protobuf/shared/organization/organization.proto`

## Usage

- Services should import these contracts directly during build.
- Keep gRPC + `google.api.http` annotations aligned with gateway routes.
- When changing request/response contracts, update service tests and gateway mapping.

## Conventions

- Prefer additive changes to maintain compatibility.
- Use clear enum values and avoid reusing numeric tags.
