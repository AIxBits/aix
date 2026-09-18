# Security

AIX is experimental and has not undergone an independent security audit. Phase 3 executes bounded workflows over registered Rust operations but does not yet implement scoped host grants. HTTP, notification and AI adapters remain disabled. Do not use it as a production sandbox for untrusted applications.

Definitions are data, never executable Python, Shell or JavaScript. The Rust runtime validates definitions independently, resolves exact registered IDs and checks Operation inputs and outputs against compiled schemas. Workflow graphs and binding depth are bounded; failed workflows restore their in-memory state snapshot and successful state commits use one SQLite statement. Cancellation is cooperative between Operations. An app's requested capabilities are not host grants. External effects stay disabled until permission mediation and their host adapters are implemented. Rendered resources and redirects remain within the same permission boundary as connectors.

Model output is untrusted. The App Builder bounds provider output, validates every candidate through the Rust Runtime and limits automatic repair attempts. The provider protocol contains no API key field. Future desktop adapters must retrieve secrets from OS-backed storage and sanitize provider errors before returning them to the UI.

Do not put API keys in definitions, examples, logs or issues. Avoid posting exploitable vulnerabilities publicly. Once a GitHub remote is established, maintainers must enable private vulnerability reporting before the Beta release; no private reporting channel has been configured yet. Only the latest Beta will receive fixes during early development.
