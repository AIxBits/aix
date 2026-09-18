# Getting started

Install Node.js >=22.18 and stable Rust, then run the commands in the [README](../README.md). `npm ci` installs the locked dependencies; `npm run check` builds the schema package and runs validation tests. `cargo test --workspace` checks the Rust protocol and runtime.

Edit `examples/hello.aix.json`, then run `npm run validate -- examples/hello.aix.json`. YAML is accepted by this TypeScript command. Validation failures print diagnostic paths and exit with code 1. Input is limited to 1 MiB; duplicate YAML keys and aliases are rejected.

Run `cargo run -p aix-runtime -- validate examples/hello.aix.json` to repeat structural and semantic checks inside Rust. The Rust trust boundary currently accepts JSON; hosts must normalize validated YAML to JSON before loading it. Run `cargo run -p aix-runtime -- operations` to inspect the operation contracts.

Validate the Phase 3 example with `npm run validate -- examples/workflow.aix.json` and `cargo run -p aix-runtime -- validate examples/workflow.aix.json`. An embedding host loads the returned `AppDefinition` into `AppSession`, supplies a `MemoryStateStore` or `SqliteStateStore`, and dispatches typed `RuntimeEvent` values. See the [Workflow Spec](../specs/workflows/README.md) for binding, ordering, timer and rollback rules.

HTTP, notification and AI operations return `adapter_unavailable` until their host adapters exist. The React/Tauri desktop launch path starts in Phase 4, so Phase 3 is currently consumed as a Rust library rather than an end-user desktop executable.
