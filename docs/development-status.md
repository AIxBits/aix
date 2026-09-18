# Development status

Phase 1 established the workspace, app schema and dual JSON/YAML validator. Phase 2 adds strict Rust protocol types, authoritative runtime validation, a checked operation registry and the ten Beta operation contracts.

Implemented execution: state.get, state.set, logic.if, collection.filter, collection.sort, data.transform and time.now. HTTP, notifications and AI have schemas and capability metadata but intentionally return `adapter_unavailable`. Workflow dispatch, persistence, host permission grants, connectors and rendering remain in later phases.

The repository is published at <https://github.com/AIxBits/aix>. Private vulnerability reporting still needs to be enabled before the public Beta release.
