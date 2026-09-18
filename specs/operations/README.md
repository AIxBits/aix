# Operation Spec 0.1.0

An operation is the only executable unit accepted by the AIX Runtime. App definitions select a registered operation by ID and provide JSON input; they cannot provide implementation code. The machine-readable descriptor contract is [operation.schema.json](operation.schema.json).

Every definition contains:

| Field | Meaning |
| --- | --- |
| `id` | Stable namespaced identifier |
| `input` | JSON Schema checked before execution |
| `output` | JSON Schema checked after execution |
| `errors` | Stable operation-owned error codes and descriptions |
| `sideEffects` | Host-observable effects, including state mutation and time reads |
| `requiredPermissions` | Capabilities the app must request before a host may perform the effect |

The registry also returns runtime boundary errors: `unknown_operation`, `invalid_input`, `invalid_output`, and `operation_contract_violation`. These describe registry enforcement rather than behavior owned by an individual operation. An implementation that returns an undeclared error is converted to `operation_contract_violation`.

## Built-ins

| ID | Input | Output | Effect / capability | Phase 2 behavior |
| --- | --- | --- | --- | --- |
| `state.get` | `path` JSON Pointer | `value` | none | Reads app-local state |
| `state.set` | `path`, `value` | `value` | `state.write` | Writes an existing container or object key |
| `logic.if` | boolean `condition`, `then`, `else` | selected `value` | none | Executes without evaluating either value as code |
| `collection.filter` | `items`, bounded `predicate` | `items` | none | Supports eq/ne/order/contains comparisons |
| `collection.sort` | `items`, optional key `path` and order | `items` | none | Stable scalar sort; rejects mixed key types |
| `data.transform` | `value`, target-to-pointer `mapping` | object `value` | none | Projects data using JSON Pointers |
| `http.request` | URL, method, headers, body | status, headers, body | `network.request` / `network.request` | Returns `adapter_unavailable` until Phase 6 |
| `time.now` | empty object | `unixMs` | `time.read` | Reads host wall-clock time |
| `notification.show` | message and optional title | delivered flag | `notification.show` / `notification.show` | Returns `adapter_unavailable` until a host adapter exists |
| `ai.generate` | prompt and optional provider-neutral hints | text and optional data | `ai.generate`, `network.request` / `ai.generate` | Returns `adapter_unavailable` until a provider adapter exists |

Exact schemas are the definitions returned by `cargo run -p aix-runtime -- operations`. Schemas use JSON Schema Draft 2020-12 and reject unknown input properties. Defaults in schemas document omitted behavior; callers do not rely on schema mutation.

## Declarative data rules

Paths are RFC 6901 JSON Pointers such as `/weather/temperature`. `collection.filter` accepts only the operators `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, and `contains`. `data.transform` maps output field names to source pointers. No operation accepts a callback, expression language, template code, JavaScript, Python or shell command.

`state.set` is confined to the current `OperationContext`. External operations have no adapter in Phase 2 and therefore cannot perform an effect. Phase 5 inserts scoped permission resolution before host effects; Phase 6 supplies the HTTP adapter. Model credentials and provider-specific clients remain outside definitions and the core registry.

## Checked invocation

`Runtime::execute(id, input, context)` performs this sequence:

1. Resolve an exact registered ID.
2. Validate input against the compiled input schema.
3. Call the Rust implementation.
4. Verify any returned error was declared.
5. Validate successful output against the compiled output schema.

App-local state is restored when an implementation returns an error or invalid output. This protects the in-memory context from partial mutations; external effects are not generally reversible and therefore require permission and adapter controls before invocation.

Workflow step inputs are also checked while loading Phase 2 app definitions. Phase 3 will resolve state and prior-step bindings first, then apply the same invocation boundary to the resolved JSON.
