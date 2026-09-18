# App Spec 0.1.0 (experimental)

The normative structural contract is [app.schema.json](../../packages/aix-schema/app.schema.json), JSON Schema Draft 2020-12. JSON and YAML map to the same data model.

Required fields: `specVersion`, `metadata`, `state`, `resources`, `ui`, `workflows`, `connectors`, `permissions`. Empty collections are valid. Unknown structural fields are rejected; JSON state, operation input, resource value and UI props remain extensible pending later-phase contracts.

Metadata contains id, name, version and an optional description. UI nodes have unique ids and a supported type. Workflows have unique ids, a trigger (`on`), an entry step and steps with id, operation, input and optional next edges. Validation rejects missing entry/edge references, cycles, duplicate step ids, missing UI targets and timers without positive bounded intervals (minimum 100 ms).

The JSON Schema checks shape; the exported TypeScript `validateApp` also checks graph semantics. `parseApp` parses bounded JSON/YAML without executing expressions. Success does not prove an operation is registered or a capability is granted. Those checks belong to the runtime.

See the [minimal example](../../examples/hello.aix.json). `$state` binding syntax is reserved for Phase 3, and is not evaluated in Phase 1.
