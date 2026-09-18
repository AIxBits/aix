# Security

AIX is experimental and has not undergone an independent security audit. Phase 1 only validates definitions and provides a non-executing Rust skeleton. Do not use it as a sandbox for untrusted code.

Definitions are data, never executable Python, Shell or JavaScript. Runtime execution must validate definitions independently, allow only registered operations and mediate every side effect. An app's requested capabilities are not host grants. Rendered resources and redirects are within the same permission boundary as connectors.

Do not put API keys in definitions, examples, logs or issues. Avoid posting exploitable vulnerabilities publicly. Once a GitHub remote is established, maintainers must enable private vulnerability reporting before the Beta release; no private reporting channel has been configured yet. Only the latest Beta will receive fixes during early development.
