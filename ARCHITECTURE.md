# Architecture

## Authoring and execution boundaries

```text
User requirement -> App Builder -> AI provider adapter
                         |              ^ secret resolved by host
                         v
                 candidate App Definition
                         |
                         v
                 validator / repair loop
                         |
                         v
                 user review and save
                         |
                         v
                  JSON/YAML App Definition
    -> structural + semantic validator
    -> Rust runtime / event dispatcher
    -> workflow graph / operation registry
    -> permission resolver -> connector or host adapter
    -> state update -> renderer snapshot
UI event -> runtime event dispatcher
```

Model output and app definitions are untrusted data. The authoring plane can generate and revise definitions, but only the runtime can execute registered operations. No eval, script node, dynamic module loading or generated host command is permitted. Runtime `ai.generate` is an app operation and is separate from the App Builder used to author apps.

## Module ownership

| Module | Responsibility |
| --- | --- |
| aix-authoring | Provider-neutral definition generation, validation and repair loop |
| aix-core | Portable app and operation protocol types |
| aix-runtime | Operation registry, state, event queue and workflow execution |
| aix-permission | Requested capabilities intersected with host-approved grants |
| aix-connector | HTTP transport and bounded OpenAPI import |
| aix-schema | JSON Schema, parsing and structural/semantic validation |
| aix-ui | React rendering of snapshots; emits typed UI events |
| desktop | Tauri 2 host, approval UI, SQLite and native adapters |

The Rust runtime is an independent trust boundary. It decodes strict protocol types, validates IDs and workflow graphs, checks registered operations and validates static operation inputs. Frontend validation cannot authorize native execution.

The authoring crate never handles credentials. A desktop host owns provider profiles, resolves keys from OS-backed secret storage and injects an `AppGenerationProvider`. Provider responses are size-bounded and cannot become runnable until the Runtime accepts them. Generated permissions are requests shown to the user, not grants.

## State and workflows (planned)

State uses JSON values. State writes flow through the runtime and publish immutable snapshots. SQLite persists app-local state and host configuration; secrets remain outside definitions. Each workflow is a finite directed acyclic graph with an explicit entry node. Phase 2 validates this graph but does not dispatch it. Phase 3 execution will be sequential and deterministic, with explicit failure propagation, step/time limits and cancellation. Event values and step outputs are data, never executable expressions.

Bindings use structured references such as `{ "$state": "city" }`; transforms and conditions use a bounded declarative vocabulary. Exact input/output contracts are introduced alongside implementation, not assumed from free-form strings.

## Side effects and trust (planned)

Every side effect, including state mutation, passes a resolver boundary. Internal app-state effects receive an app-local runtime grant; external effects require host-approved scoped capabilities. Requested permissions are not granted permissions. Check the resolved destination at execution time, including redirects and resource loading. UI media fetches cannot bypass network policy; the host resolves resources into safe handles.

Resources have identity and content independent of UI nodes. Renderers receive resource handles; connectors return structured data. React contains no weather logic and no direct API client.

## Beta scope

One host, one process, a small operation registry, DAG workflows and basic UI components. No arbitrary plugins, distributed scheduler, code sandbox or provider-specific core abstractions. Tauri integration is deferred to Phase 4; HTTP transport to Phase 6.
