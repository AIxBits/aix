# Render contract 0.1.0 (experimental)

Supported node types are page, container, text, button, input, list, table, image, audio and video. Nodes contain id, type, optional props and children. IDs are app-wide unique.

The renderer consumes state snapshots and resource handles. `$state` references bind values; UI interactions produce ui.click/ui.change events addressed by node id. The runtime owns workflow lookup and execution. No eval, inline scripts, injected HTML or direct business API calls are allowed.

Current props are deliberately small:

- page/container: optional `label`; children contain layout content.
- text: scalar `text` or a text `resource` id.
- button: `label`, optional `disabled`; click emits an empty payload.
- input: `value`, `label`, `placeholder`; change emits `{ "value": string }`.
- list: an `items` array or data `resource`.
- table: a `rows` array or data `resource`, plus optional string `columns`.
- image/audio/video: a matching `resource` id; image also accepts `alt`.

Props may contain bounded exact `{ "$state": "/json/pointer" }` bindings. Missing values render empty. Bindings cannot call functions or construct expressions. Lists and tables stringify nested JSON as text; the renderer never uses injected HTML.
