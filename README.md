# AIX — AI-native Application Runtime

AIX is an open specification and runtime for applications described by AI or people. AI builds a definition; the runtime validates and executes registered operations. Definitions never execute arbitrary Python, Shell, or JavaScript.

**Status: Phase 1 foundation.** JSON/YAML validation and a Rust executable are available. Runtime operations, React rendering, Tauri integration, SQLite persistence and the weather demo are planned, not implemented. This is not yet a production runtime or the completed Beta.

## Quick start

Requires Node.js >=22.18, npm and stable Rust.

```sh
npm ci
npm run check
npm run validate -- examples/hello.aix.json
cargo test --workspace
cargo run -p aix-runtime
```

The validator prints `Valid AIX App`; the Rust executable prints the supported specification version. Neither executes the example yet.

## Repository

- `crates/`: UI-independent Rust protocol, runtime, permissions and connectors.
- `packages/aix-schema/`: versioned JSON Schema and JSON/YAML validator.
- `packages/aix-ui/`, `apps/desktop/`: reserved React renderer and Tauri host boundaries.
- `specs/`: public contracts; `examples/`: application definitions.
- `tests/`: schema integration tests; crate modules contain Rust unit tests.

See [Architecture](ARCHITECTURE.md), [Roadmap](ROADMAP.md), [Getting started](docs/getting-started.md), [App Spec](specs/app/README.md), [Operations](specs/operations/README.md), [Permissions](specs/permissions/README.md), and [Connectors](specs/connectors/README.md).

Contributions follow [CONTRIBUTING](CONTRIBUTING.md). Security boundaries and reporting are in [SECURITY](SECURITY.md). Licensed under Apache-2.0.
