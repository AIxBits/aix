# Desktop host

Tauri 2 + React host for AIX. Paste a JSON App Definition, load it through the Rust Runtime, and interact with the declarative renderer. UI events return through typed IPC to `AppSession`; React cannot execute Operations directly.

Use `npm run desktop:dev -w @aix/desktop` after installing Tauri platform prerequisites. Phase 7 adds provider profiles, secret storage and the AI App Builder interface.
