# Getting started

Install Node.js >=22.18 and stable Rust, then run the commands in the [README](../README.md). `npm ci` installs the locked dependencies; `npm run check` builds the schema package and runs validation tests. `cargo test --workspace` checks Rust modules, and `cargo run -p aix-runtime` runs the skeleton.

Edit `examples/hello.aix.json`, then run `npm run validate -- examples/hello.aix.json`. YAML is accepted by the same command. Validation failures print diagnostic paths and exit with code 1. Input is limited to 1 MiB; duplicate YAML keys and aliases are rejected.

There is no desktop launch command in Phase 1. Installing Tauri platform dependencies and configuring SQLite belong to later phases. Do not interpret successful validation as permission to execute an application.
