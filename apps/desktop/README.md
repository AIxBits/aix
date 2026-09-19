# Desktop host

Tauri 2 + React host for AIX. Build an App Definition through a saved OpenAI-compatible provider profile or paste JSON directly, review requested capability scopes, load it through the Rust Runtime, and interact with the declarative renderer. Provider keys are stored in the OS credential store; profile metadata is stored in SQLite. Generated definitions are Runtime-validated before review and can be exported as JSON or YAML. The host constructs grants only from explicitly selected requested scopes. UI events return through typed IPC to `AppSession`; React cannot execute Operations directly.

Use `npm run desktop:dev -w @aix/desktop` after installing Tauri platform prerequisites. Open **Provider setup**, add an HTTPS OpenAI-compatible endpoint or a loopback local endpoint, then enter a requirement. Exported definitions are written under `Downloads/AIX` when available.
