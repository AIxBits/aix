# AI App Authoring

The Authoring Plane turns a user's natural-language requirement into a complete AIX App Definition. It is separate from the Runtime Plane: authoring creates untrusted data, while the runtime validates and executes only registered operations.

```text
requirement -> AppBuilder -> AppGenerationProvider -> candidate JSON
                    ^                                |
                    |------ validator issues --------|
                    |
                    -> validated definition -> user review -> save/run
```

## Rust API

`aix-authoring` exposes:

- `AppGenerationProvider`: asynchronous provider adapter injected by the host.
- `ProviderRequest`: credential-free system/user messages and generate/revise/repair purpose.
- `AppBuilder`: bounded generation, Runtime validation, normalized JSON output and limited repair.
- `GenerateAppRequest`: natural-language requirement plus an optional previously validated definition.
- `GeneratedApp`: accepted typed definition, normalized JSON source and provider attempt count.

The initial call supplies the public App Schema and registered Operation definitions to the provider. The response must contain one complete JSON object. A single `json` Markdown fence is tolerated, then removed. The Runtime performs strict decoding, graph validation, Operation lookup, static input validation and required-capability checks.

Invalid candidates can be returned to the same provider with structured validation issues. Default repair count is two and the hard maximum is three. Requirements are limited to 64 KiB and provider output to 1 MiB. Oversized output is rejected without copying it into a repair prompt.

The desktop product supplies a real OpenAI chat-completions compatible adapter. Embedders can use the interface as follows:

```rust,ignore
let builder = AppBuilder::new(provider)?;
let generated = builder.generate(GenerateAppRequest {
    requirement: "Build a weather app".into(),
    existing_definition: None,
}).await?;
save_after_user_review(generated.source);
```

## Provider profiles and secrets

The desktop host owns profiles containing a profile ID, provider kind, API base URL and default model. Profile metadata lives in SQLite. The API key itself lives in OS-backed secret storage. It is resolved inside the host adapter and is never placed in `ProviderRequest`, App Definitions, prompts, ordinary SQLite fields or logs. The password input is uncontrolled and cleared after submission so the UI does not retain the key in application state.

The `open_ai_compatible` profile requires an HTTPS endpoint and a key. The `local_open_ai_compatible` profile permits HTTPS or loopback-only HTTP and may omit the key. A base URL is expanded with `/chat/completions` unless it already names that endpoint. Requests have a 60-second timeout, reject redirects, accept at most 1 MiB of response data and return sanitized errors. The profile selected by the user fixes the destination; model output cannot change it. The App Builder receives the adapter, not its credentials.

## User control

Before saving or running generated output, the desktop UI shows a bounded JSON Pointer change list and requested permissions. A revision includes the previous Runtime-validated definition and repeats the same validation loop. Export revalidates the source and writes JSON or YAML under `Downloads/AIX` where the platform provides a Downloads directory. The user separately grants runtime capabilities when loading. A model cannot grant its generated app access to network, files, notifications or AI providers.

Runtime `ai.generate` serves a different purpose: it lets an already running app request model output through an authorized workflow. It is not used as the trusted implementation of App Builder generation.
