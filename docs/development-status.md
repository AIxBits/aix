# Development status

Phase 1 establishes the workspace, schema, validator and runnable Rust skeleton. No runtime execution or desktop renderer is implemented yet.

Local verification uses an isolated Rust installation in `/tmp/aix-cargo` and `/tmp/aix-rustup`, because the machine had no Rust toolchain. To reuse it in this development session:

```sh
export CARGO_HOME=/tmp/aix-cargo
export RUSTUP_HOME=/tmp/aix-rustup
export PATH="$CARGO_HOME/bin:$PATH"
cargo run -p aix-runtime --locked
```

These temporary directories may be cleaned by the operating system. Other contributors should install stable Rust normally. No global shell configuration was changed.

No Git remote is configured. CI is committed for future GitHub pushes; hosted CI has not run. Private security reporting must be configured when publishing the repository.

Verification completed: TypeScript build; 10 schema tests; JSON/YAML example validation; built JavaScript package import; Rust executable; 2 Rust unit tests; rustfmt and clippy with warnings denied.
