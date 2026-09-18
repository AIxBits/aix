# Contributing

Keep changes small and follow the phase gates in ROADMAP.md. Explain the user-visible problem, module boundary, tests and security implications in each pull request.

Before submitting, run `npm ci`, `npm run check`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. Update public specifications whenever a contract changes. Do not commit secrets, dependency build outputs or local databases.

Core APIs need useful doc comments. Runtime changes must not depend on React, a specific model provider or arbitrary generated code execution. Add denial/error-path tests for new external effects. Use clear commit messages such as `feat(schema): validate app definitions`.
