# libs

Shared Rust workspace for reusable Philand libraries.

## Crates

- Core: `configs`, `logging`, `error`, `http`, `validator`, `env`, `time`, `random`, `crypto`
- Data: `storage`, `table`
- Integration: `oauth2`, `notify`, `queue`, `socket`, `ssh`, `application`
- Utilities: `common`, `concurrency`, `marshaler`, `copier`, `ratelimit`

## Workspace Commands

```bash
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Storage Provider Pattern

- `storage` is a shared database provider layer.
- Service repos (for example `identity/src/manager/repository`) implement entity-specific logic and call provider functions.
- Table constants live in `table/src/table.rs`.

See migration planning docs:

- `docs/go-port-inventory.md`
- `docs/go-to-rust-roadmap.md`
