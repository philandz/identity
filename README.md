# identity

Identity service for the Philand microservices platform.

## Responsibilities

- User registration and login
- JWT profile/auth flows (`/login`, `/profile`, `/refresh`, `/logout`)
- Password operations (`/update`, `/forgot`, `/reset`)
- Organization IAM (members, invitations, role changes)
- gRPC service plus REST endpoints (Axum)

## Runtime Endpoints

- gRPC: `GRPC_HOST:GRPC_PORT` (default `127.0.0.1:50051`)
- HTTP: `HTTP_HOST:HTTP_PORT` (default `127.0.0.1:3001`)
- Health: `GET /health`
- OpenAPI JSON: `GET /api-docs/openapi.json`
- Swagger UI: `GET /docs`

## Local Run

Prerequisites:

- Rust stable toolchain
- MySQL running and reachable

Start service:

```bash
cargo run
```

Required env:

- `DATABASE_URL`
- `JWT_SECRET`

See shared config contract in `../libs/configs/README.md` for all supported variables and defaults.

## Testing

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

Manual e2e via gateway:

```bash
bash ../scripts/identity_manual_test.sh
```

## Architecture Notes

- Service repository code lives under `src/manager/repository`.
- Database provider helpers come from `../libs/storage`.
- Shared contracts are in `../protobuf`.
