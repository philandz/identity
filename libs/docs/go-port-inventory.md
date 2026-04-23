# Go -> Rust Port Inventory

Source repositories analyzed:

- `/Users/phileanh/go/src/git.infiniband.vn/cloud/libs/pkcore`
- `/Users/phileanh/go/src/git.infiniband.vn/cloud/libs/biz`

## Target Rust Workspace Layout

- `configs`
- `logging`
- `crypto`
- `oauth2`
- `storage`
- `http`
- `ratelimit`
- `concurrency`
- `random`
- `ssh`
- `time`
- `notify`
- `queue`
- `marshaler`
- `copier`
- `socket`
- `error`
- `env`
- `application`
- `validator`
- `common`

## Mapping Summary

### Priority (Phase 1)

| Go package family | Rust crate | Notes |
| --- | --- | --- |
| `pkcore/config` + config helpers | `configs` | Env/K8s-driven config structs |
| `pkcore/logging/*` | `logging` | Use tracing only |
| `pkcore/crypto/*`, `biz/helper/crypto/password` | `crypto` | Hashing + password helpers |
| `biz/helper/oauth2/*` (Google first) | `oauth2` | Auth code + refresh flow |
| `pkcore/storage/mysql`, `pkcore/storage/s3` | `storage` | MySQL via sqlx, S3 + presigned URLs |

### Secondary (Phase 2+)

| Go package family | Rust crate |
| --- | --- |
| `pkcore/http`, `pkcore/http/client` | `http` |
| `pkcore/ratelimit/*` | `ratelimit` |
| `pkcore/concurrency/*`, `biz/helper/sync` | `concurrency` |
| `pkcore/random` | `random` |
| `pkcore/ssh` | `ssh` |
| `pkcore/time` | `time` |
| `pkcore/notify/*` | `notify` |
| `pkcore/queue/*` | `queue` |
| `pkcore/marshaler` | `marshaler` |
| `pkcore/copier`, `pkcore/converter`, `biz/helper/mapslice` | `copier` |
| `pkcore/socket/*` | `socket` |
| `pkcore/error` | `error` |
| `pkcore/environment` | `env` |
| `pkcore/application/*` | `application` |
| `biz/helper/validator` | `validator` |
| `biz/helper/common`, `biz/helper/resource`, `biz/helper/filter`, `biz/helper/event`, `biz/helper/generate` | `common` |

## Compatibility Targets

- Feature parity with Go implementations where practical.
- Similar runtime behavior and error semantics in Rust.
- Prefer safe defaults and clearer typing in Rust APIs.
