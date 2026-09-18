# Roadmap

Each phase must build, add meaningful tests, update documentation and land in its own Git commit before the next phase starts.

- [x] **Phase 1 — Skeleton and schema:** workspace, JSON/YAML validation, semantic graph checks, runnable Rust stub, initial docs and CI. Acceptance: npm checks, example validation, cargo tests and executable pass.
- [x] **Phase 2 — Operation runtime:** authoritative Rust validation; registry and uniform id/input/output/errors/side-effects/permissions descriptors; state.get/set, logic.if, collection.filter/sort, data.transform, http.request, time.now, notification.show and ai.generate. Host-dependent operations initially fail explicitly when adapters are unavailable. Test unknown operations, invalid inputs and failures.
- [x] **Phase 2.5 — AI authoring foundation:** provider-neutral App Builder API, credential-free provider protocol, bounded generation output, Runtime validation and limited validator-driven repair. No network provider or desktop UI yet.
- [ ] **Phase 3 — State and workflow:** JSON state, SQLite persistence, binding resolution, bounded DAG execution, app.start/ui.click/ui.change/timer events and cancellation. Test branching, ordering, persistence and failure propagation.
- [ ] **Phase 4 — Renderer:** React + TypeScript components (page, container, text, button, input, list, table, image, audio, video), resource handles and Tauri 2 bridge. Test bindings and event dispatch without UI business logic.
- [ ] **Phase 5 — Permission system:** scoped host grants for ai.generate/network.request/file.read/file.write/notification.show; deny by default and mediate all side effects. Test path escapes, URL mismatches, denied resources and forged grants.
- [ ] **Phase 6 — HTTP connector:** bounded requests, redirect revalidation, response validation, operation conversion and a documented OpenAPI subset. Test against a local fixture server, including denial and error cases.
- [ ] **Phase 7 — AI App Builder product:** OpenAI-compatible and local-provider adapters, user provider profiles, OS-backed secret storage, generation/revision UI, definition diff, permission preview and save to JSON/YAML. Provider calls pass scoped host controls.
- [ ] **Phase 8 — Weather demo:** generate or load the app; input city -> event -> workflow -> permission -> weather HTTP API -> state -> text/image; Refresh repeats the same runtime path. Include deterministic offline fixtures and optional live API mode.
- [ ] **Phase 9 — Public Beta:** complete API/spec docs, end-to-end tests, supported-platform CI, contribution and security review, release checklist and GitHub repository setup if a remote is supplied.

No subsequent phase is implied complete by an empty module or a schema field. The schema is experimental until the end-to-end Beta establishes its contracts.
