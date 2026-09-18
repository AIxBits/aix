# Render contract (Phase 4 design)

Supported node types are page, container, text, button, input, list, table, image, audio and video. Nodes contain id, type, optional props and children. IDs are app-wide unique. Component-specific props will be constrained as components are implemented.

The renderer consumes state snapshots and resource handles. `$state` references bind values; UI interactions produce ui.click/ui.change events addressed by node id. The runtime owns workflow lookup and execution. No eval, inline scripts, injected HTML or direct business API calls are allowed.
