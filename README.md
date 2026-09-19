# AIX — AI-native Application Runtime

AIX is an open specification and runtime for applications described by AI or people. AI builds a definition; the runtime validates and executes registered operations. Definitions never execute arbitrary Python, Shell, or JavaScript.

**Status: Phase 5 foundation.** App definitions are validated independently in TypeScript and Rust. The Runtime executes bounded event-driven DAG workflows, persists app-local state through SQLite and checks every effect at a permission boundary. External capabilities are denied unless a separately approved, app-bound scope and the definition request both cover the concrete target. The desktop host displays requested scopes before activation. The React renderer supports all ten Beta nodes, and the provider-neutral App Builder can generate, validate and repair definitions through an injected model adapter. Real provider connections, credential storage, HTTP transport and the weather demo are not implemented yet. This is not a production runtime or the completed Beta.

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
npm run desktop:dev -w @aix/desktop
```

The TypeScript command accepts JSON or YAML. The Rust command validates JSON at the runtime trust boundary. `operations` prints every registered input/output schema, error, side effect and required capability. The desktop command opens the Tauri host; paste [workflow.aix.json](examples/workflow.aix.json) into its loader to exercise `Definition -> Runtime -> State -> Renderer -> Event`. Definitions with permissions enter a separate review screen; unchecked scopes remain denied.

## Repository

- `crates/`: UI-independent Rust authoring, protocol, runtime, permissions and connectors.
- `packages/aix-schema/`: versioned JSON Schema and JSON/YAML validator.
- `packages/aix-ui/`, `apps/desktop/`: React renderer and Tauri 2 host.
- `specs/`: public contracts; `examples/`: application definitions.
- `tests/`: schema integration tests; crate modules contain Rust unit tests.

See [Architecture](ARCHITECTURE.md), [Roadmap](ROADMAP.md), [Getting started](docs/getting-started.md), [Authoring](specs/authoring/README.md), [App Spec](specs/app/README.md), [Workflows](specs/workflows/README.md), [Operations](specs/operations/README.md), [Permissions](specs/permissions/README.md), and [Connectors](specs/connectors/README.md).

Contributions follow [CONTRIBUTING](CONTRIBUTING.md). Security boundaries and reporting are in [SECURITY](SECURITY.md). Licensed under Apache-2.0.
