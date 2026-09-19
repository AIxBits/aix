# Development status

Phase 1 established the workspace, app schema and dual JSON/YAML validator. Phase 2 added strict Rust protocol types, authoritative runtime validation, a checked operation registry and the ten Beta operation contracts. Phase 2.5 adds the provider-neutral App Builder contract and validation-driven repair loop.

Implemented execution: state.get, state.set, logic.if, collection.filter, collection.sort, data.transform and time.now. HTTP, notifications and AI have schemas and now require scoped host grants before they intentionally return `adapter_unavailable`. Workflow dispatch, SQLite persistence, React rendering, Tauri permission review and the capability resolver are implemented. HTTP transport and connector conversion remain in later phases.

The authoring crate accepts an injected model provider and can generate or revise a complete App Definition, normalize valid JSON and retry rejected output with structured validator issues. It does not yet connect to a real provider, accept API keys or expose a desktop generation screen.

The repository is published at <https://github.com/AIxBits/aix>. Private vulnerability reporting still needs to be enabled before the public Beta release.
