# Security

AIX is experimental and has not undergone an independent security audit. Phase 6 executes bounded workflows, enforces scoped host grants and provides a bounded HTTP(S) adapter. Notification and AI adapters remain disabled. Do not use it as a production sandbox for untrusted applications.

Definitions are data, never executable Python, Shell or JavaScript. The Rust runtime validates definitions independently, resolves exact registered IDs and checks Operation inputs and outputs against compiled schemas. Workflow graphs and binding depth are bounded; failed workflows restore their in-memory state snapshot and successful state commits use one SQLite statement. Cancellation is cooperative between Operations. An app's requested capabilities are not host grants. HTTP targets and every redirect are authorized immediately before sending; cross-origin redirects lose sensitive headers. Request, response, URL, header, redirect and time limits are host-owned. DNS resolution and connection still occur in the underlying HTTP library, so AIX should not be treated as an audited SSRF sandbox.

React receives an immutable UI/state/resource snapshot and can only return typed events. The Tauri bridge exposes no direct Operation command. UI text is rendered as React text, and remote Definition media is not assigned to DOM URLs. The current content security policy limits content to packaged assets and approved inline media.

Model output is untrusted. The App Builder bounds provider output, validates every candidate through the Rust Runtime and limits automatic repair attempts. The provider protocol contains no API key field. Future desktop adapters must retrieve secrets from OS-backed storage and sanitize provider errors before returning them to the UI.

Do not put API keys in definitions, examples, logs or issues. Avoid posting exploitable vulnerabilities publicly. Once a GitHub remote is established, maintainers must enable private vulnerability reporting before the Beta release; no private reporting channel has been configured yet. Only the latest Beta will receive fixes during early development.
