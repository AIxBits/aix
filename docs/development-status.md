# Development status

Phase 1 established the workspace, app schema and dual JSON/YAML validator. Phase 2 added strict Rust protocol types, authoritative runtime validation, a checked operation registry and the ten Beta operation contracts. Phase 2.5 added the provider-neutral App Builder contract and validation-driven repair loop. Phase 7 connects that contract to the desktop product.

Implemented execution: state.get, state.set, logic.if, collection.filter, collection.sort, data.transform, time.now and bounded HTTP(S). Declarative HTTP connectors are registered as Operations, and the local OpenAPI 3 subset importer emits connector declarations. Notification and AI operations still return `adapter_unavailable` after permission approval. Workflow dispatch, SQLite persistence, React rendering, Tauri permission review and the capability resolver are implemented.

The desktop host provides OpenAI-compatible hosted and local adapters, profile persistence, OS-backed secrets, a generation/revision screen, structural diff, permission preview and JSON/YAML export. Model output stays untrusted until the Rust Runtime accepts it. Phase 8 will use this path to build and run the complete weather demo.

The repository is published at <https://github.com/AIxBits/aix>. Private vulnerability reporting still needs to be enabled before the public Beta release.
