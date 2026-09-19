# Getting started

Install Node.js >=22.18 and stable Rust, then run the commands in the [README](../README.md). `npm ci` installs the locked dependencies; `npm run check` builds the schema package and runs validation tests. `cargo test --workspace` checks the Rust protocol and runtime.

Edit `examples/hello.aix.json`, then run `npm run validate -- examples/hello.aix.json`. YAML is accepted by this TypeScript command. Validation failures print diagnostic paths and exit with code 1. Input is limited to 1 MiB; duplicate YAML keys and aliases are rejected.

Run `cargo run -p aix-runtime -- validate examples/hello.aix.json` to repeat structural and semantic checks inside Rust. The Rust trust boundary currently accepts JSON; hosts must normalize validated YAML to JSON before loading it. Run `cargo run -p aix-runtime -- operations` to inspect the operation contracts.

Validate the Phase 3 example with `npm run validate -- examples/workflow.aix.json` and `cargo run -p aix-runtime -- validate examples/workflow.aix.json`. An embedding host loads the returned `AppDefinition` into `AppSession`, supplies a `MemoryStateStore` or `SqliteStateStore`, and dispatches typed `RuntimeEvent` values. See the [Workflow Spec](../specs/workflows/README.md) for binding, ordering, timer and rollback rules.

To start the desktop host, install the [platform prerequisites listed by Tauri](https://v2.tauri.app/start/prerequisites/) and run:

```sh
npm run desktop:dev -w @aix/desktop
```

Paste the contents of `examples/workflow.aix.json` and choose **Review and load**. Loading runs `app.start`; changing the city emits `ui.change`; the Refresh button emits `ui.click`. Every event crosses Tauri IPC and returns a new Runtime snapshot. App-local state is stored in the operating system's AIX application-data directory.

To inspect Phase 5, load `examples/permission-test.aix.json`. The desktop displays its `notification.show` scope unchecked. Loading without selecting it creates no grant, so the button produces `permission_denied`; selecting it creates an app-bound grant, so the same button reaches the notification boundary and currently produces `adapter_unavailable`. `cargo test -p aix-permission` runs the URL origin/path and file traversal/symlink checks, while `cargo test -p aix-runtime host_operations_require_grants_before_reaching_adapters` demonstrates pre-adapter enforcement.

Use `npm run desktop:build -w @aix/desktop` to produce the current operating system's native package. Packaging and signing configuration for all supported release targets is completed in Phase 9.

After permission checks pass, HTTP, notification and AI operations return `adapter_unavailable` until their host adapters exist.
