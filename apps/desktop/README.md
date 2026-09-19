# Desktop host

Tauri 2 + React host for AIX. Paste a JSON App Definition, review its requested capability scopes, load it through the Rust Runtime, and interact with the declarative renderer. The host constructs grants only from explicitly selected requested scopes. UI events return through typed IPC to `AppSession`; React cannot execute Operations directly.

Use `npm run desktop:dev -w @aix/desktop` after installing Tauri platform prerequisites. Phase 7 adds provider profiles, secret storage and the AI App Builder interface.
