# AIX — AI-native Application Runtime

AIX is an open specification and runtime for applications described by AI or people. AI builds a definition; the runtime validates and executes registered operations. Definitions never execute arbitrary Python, Shell, or JavaScript.

**Status: Phase 7 foundation.** App definitions are validated independently in TypeScript and Rust. The Runtime executes bounded event-driven DAG workflows, persists app-local state through SQLite and checks every effect at a permission boundary. HTTP(S) requests use a bounded host adapter, and declarative REST connectors become registered Operations. The desktop AI App Builder now connects to OpenAI-compatible hosted or local providers, stores provider keys in the operating-system credential store, validates and repairs model output, displays definition changes and requested permissions, and exports reviewed JSON or YAML. The complete weather demo is the next phase. This is not a production runtime or the completed Beta.

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
cargo run -p aix-runtime -- import-openapi openapi.json connector-id
npm run desktop:dev -w @aix/desktop
```

The TypeScript command accepts JSON or YAML. The Rust command validates JSON at the runtime trust boundary. `operations` prints every built-in input/output schema, error, side effect and required capability. `import-openapi` converts the documented local OpenAPI subset into a connector declaration. The desktop command opens the Tauri host. Configure a hosted OpenAI-compatible API or a loopback local server in **Provider setup**, describe an app, review the validated result, then run or export it. You can also paste [workflow.aix.json](examples/workflow.aix.json) into **Load JSON**. Definitions with permissions enter a separate review screen; unchecked scopes remain denied.

## Repository

- `crates/`: UI-independent Rust authoring, protocol, runtime, permissions and connectors.
- `packages/aix-schema/`: versioned JSON Schema and JSON/YAML validator.
- `packages/aix-ui/`, `apps/desktop/`: React renderer and Tauri 2 host.
- `specs/`: public contracts; `examples/`: application definitions.
- `tests/`: schema integration tests; crate modules contain Rust unit tests.

See [Architecture](ARCHITECTURE.md), [Roadmap](ROADMAP.md), [Getting started](docs/getting-started.md), [AI App Builder](docs/app-builder.md), [Authoring](specs/authoring/README.md), [App Spec](specs/app/README.md), [Workflows](specs/workflows/README.md), [Operations](specs/operations/README.md), [Permissions](specs/permissions/README.md), and [Connectors](specs/connectors/README.md).

Contributions follow [CONTRIBUTING](CONTRIBUTING.md). Security boundaries and reporting are in [SECURITY](SECURITY.md). Licensed under Apache-2.0.
