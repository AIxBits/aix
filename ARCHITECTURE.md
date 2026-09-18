# Architecture

## Execution boundary

```text
User / AI provider
    -> JSON or YAML App Definition
    -> structural + semantic validator
    -> Rust runtime / event dispatcher
    -> workflow graph / operation registry
    -> permission resolver -> connector or host adapter
    -> state update -> renderer snapshot
UI event -> runtime event dispatcher
```

The app is untrusted data. Only registered operations execute. No eval, script node, dynamic module loading or generated host command is permitted. An AI provider is an optional adapter behind `ai.generate`, never an execution engine.

## Module ownership

| Module | Responsibility |
| --- | --- |
| aix-core | Portable app and operation protocol types |
| aix-runtime | Operation registry, state, event queue and workflow execution |
| aix-permission | Requested capabilities intersected with host-approved grants |
| aix-connector | HTTP transport and bounded OpenAPI import |
| aix-schema | JSON Schema, parsing and structural/semantic validation |
| aix-ui | React rendering of snapshots; emits typed UI events |
| desktop | Tauri 2 host, approval UI, SQLite and native adapters |

Current Phase 1 Rust types are envelopes, not proof of validation. The Rust runtime must implement authoritative validation before accepting runnable definitions in Phase 2. Frontend validation alone cannot authorize native execution.

## State and workflows (planned)

State uses JSON values. State writes flow through the runtime and publish immutable snapshots. SQLite persists app-local state and host configuration; secrets remain outside definitions. Each workflow is a finite directed acyclic graph with an explicit entry node. Initial execution will be sequential and deterministic, with explicit failure propagation, step/time limits and cancellation. Event values and step outputs are data, never executable expressions.

Bindings use structured references such as `{ "$state": "city" }`; transforms and conditions use a bounded declarative vocabulary. Exact input/output contracts are introduced alongside implementation, not assumed from free-form strings.

## Side effects and trust (planned)

Every side effect, including state mutation, passes a resolver boundary. Internal app-state effects receive an app-local runtime grant; external effects require host-approved scoped capabilities. Requested permissions are not granted permissions. Check the resolved destination at execution time, including redirects and resource loading. UI media fetches cannot bypass network policy; the host resolves resources into safe handles.

Resources have identity and content independent of UI nodes. Renderers receive resource handles; connectors return structured data. React contains no weather logic and no direct API client.

## Beta scope

One host, one process, a small operation registry, DAG workflows and basic UI components. No arbitrary plugins, distributed scheduler, code sandbox or provider-specific core abstractions. Tauri integration is deferred to Phase 4; HTTP transport to Phase 6.
