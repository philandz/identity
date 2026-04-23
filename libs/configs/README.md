## Config Contract

Shared config parsing lives in this crate and is the source of truth for service env behavior.

### Identity (`IdentityServiceConfig`)

| Env var | Required | Default | Notes |
| --- | --- | --- | --- |
| `DATABASE_URL` | yes | none | Fail-fast if missing |
| `JWT_SECRET` | yes | none | Fail-fast if missing |
| `GRPC_HOST` | no | `127.0.0.1` | gRPC bind host |
| `GRPC_PORT` | no | `50051` | gRPC bind port |
| `HTTP_HOST` | no | `127.0.0.1` | HTTP bind host |
| `HTTP_PORT` | no | `3001` | HTTP bind port |
| `CONSUL_ADDR` | no | `http://127.0.0.1:8500` | Consul endpoint |
| `SERVICE_NAME` | no | `identity` | Consul registration name |
| `SUPER_ADMIN_EMAIL` | no | `laphi1612@gmail.com` | bootstrap admin |
| `SUPER_ADMIN_PASSWORD` | no | `Aa@123456` | bootstrap admin |

### Gateway (`GatewayServiceConfig`)

| Env var | Required | Default | Notes |
| --- | --- | --- | --- |
| `UPSTREAM_URL` | yes | none | Monolith upstream |
| `IDENTITY_GRPC_URL` | yes | none | Identity gRPC endpoint |
| `IDENTITY_URL` | no | `http://127.0.0.1:3001` | Identity HTTP fallback |
| `IDENTITY_TRANSPORT` | no | `grpc_transcode` | `grpc_transcode` or fallback to proxy mode |
| `HOST` | no | `0.0.0.0` | Gateway bind host |
| `PORT` | no | `3000` | Gateway bind port |
