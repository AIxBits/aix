# Getting started

Install Node.js >=22.18 and stable Rust, then run the commands in the [README](../README.md). `npm ci` installs the locked dependencies; `npm run check` builds the schema package and runs validation tests. `cargo test --workspace` checks the Rust protocol and runtime.

Edit `examples/hello.aix.json`, then run `npm run validate -- examples/hello.aix.json`. YAML is accepted by this TypeScript command. Validation failures print diagnostic paths and exit with code 1. Input is limited to 1 MiB; duplicate YAML keys and aliases are rejected.

Run `cargo run -p aix-runtime -- validate examples/hello.aix.json` to repeat structural and semantic checks inside Rust. The Rust trust boundary currently accepts JSON; hosts must normalize validated YAML to JSON before loading it. Run `cargo run -p aix-runtime -- operations` to inspect the operation contracts.

Phase 2 can invoke registered operations through the Rust `Runtime::execute` API. It does not dispatch workflows. HTTP, notification and AI operations return `adapter_unavailable` until their host adapters exist. There is no desktop launch command yet.
