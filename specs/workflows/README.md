# Workflow Spec 0.1.0 (experimental)

A workflow is a finite directed acyclic graph selected by `app.start`, `ui.click`, `ui.change` or `timer`. UI triggers identify a UI node with `target`. Timer triggers declare `intervalMs`; the host reads `AppSession::timer_subscriptions` and emits a timer event with the workflow id. The Runtime does not create background threads.

Each workflow has one `entry` and at most 256 steps. A step calls one registered Operation. `next` accepts an unconditional step id or a conditional edge:

```json
{
  "step": "showResult",
  "when": { "path": "/value", "equals": true }
}
```

The condition compares `equals` with the current step output at the JSON Pointer in `path`. Empty `path` compares the complete output. Structurally reachable steps run at most once in deterministic topological order. An edge activates its target; steps on unselected branches are skipped.

Operation input supports exact structured bindings. Bindings replace the complete JSON object in which they occur:

```json
{ "$state": "/city" }
{ "$event": "/value" }
{ "$step": "fetch", "path": "/body/temperature" }
```

State and event paths are JSON Pointers; a bare top-level state key remains accepted for the initial 0.1 examples. A step binding can read only a completed step in the same workflow. Binding results are ordinary JSON and must pass the target Operation input schema before execution.

An `AppSession` loads app-local JSON state from a `StateStore`. The desktop host will use `SqliteStateStore`; embedders can provide another implementation. A workflow takes a state snapshot before execution and persists only after every active step succeeds. Binding, Operation, cancellation, step-limit or persistence failure restores that snapshot. Cancellation is cooperative between Operation invocations.

See [workflow.aix.json](../../examples/workflow.aix.json).
