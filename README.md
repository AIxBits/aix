# AIX — AI-native Application Runtime

AIX is an open specification and runtime for applications described by AI or people. AI builds a definition; the runtime validates and executes registered operations. Definitions never execute arbitrary Python, Shell, or JavaScript.

**Status: Phase 2.5 foundation.** App definitions are validated independently in TypeScript and Rust. The Rust registry exposes checked contracts for all ten Beta operations and executes the pure/state operations. A provider-neutral App Builder can generate, validate and repair definitions through an injected model adapter. Real provider connections, credential storage, workflow scheduling, React rendering, Tauri integration, SQLite persistence and the weather demo are not implemented yet. This is not a production runtime or the completed Beta.

## Quick start

Requires Node.js >=22.18, npm and stable Rust.

```sh
npm ci
npm run check
npm run validate -- examples/hello.aix.json
cargo test --workspace
cargo run -p aix-runtime
cargo run -p aix-runtime -- validate examples/hello.aix.json
cargo run -p aix-runtime -- operations
```

The TypeScript command accepts JSON or YAML. The Rust command validates JSON at the runtime trust boundary. `operations` prints every registered input/output schema, error, side effect and required capability. Workflow execution begins in Phase 3; validating an app does not run it.

## Repository

- `crates/`: UI-independent Rust authoring, protocol, runtime, permissions and connectors.
- `packages/aix-schema/`: versioned JSON Schema and JSON/YAML validator.
- `packages/aix-ui/`, `apps/desktop/`: reserved React renderer and Tauri host boundaries.
- `specs/`: public contracts; `examples/`: application definitions.
- `tests/`: schema integration tests; crate modules contain Rust unit tests.

See [Architecture](ARCHITECTURE.md), [Roadmap](ROADMAP.md), [Getting started](docs/getting-started.md), [Authoring](specs/authoring/README.md), [App Spec](specs/app/README.md), [Operations](specs/operations/README.md), [Permissions](specs/permissions/README.md), and [Connectors](specs/connectors/README.md).

Contributions follow [CONTRIBUTING](CONTRIBUTING.md). Security boundaries and reporting are in [SECURITY](SECURITY.md). Licensed under Apache-2.0.
