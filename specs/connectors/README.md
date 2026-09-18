# Connectors (Phase 6 design)

HTTP connector declarations have id, type `http`, baseUrl and operations. Each operation declares an id, HTTP method and path. IDs must be unique across connectors. A definition registers a request mapping, never grants access.

The adapter resolves parameters, requests permission for the final destination, sends a bounded request and validates the response. It must not leak credentials across origins or follow unchecked redirects. HTTP errors, timeouts, decoding failures and denial are structured runtime errors.

A simple OpenAPI importer will support a documented subset of paths and parameters. Unsupported constructs and remote references must fail explicitly. No importer or HTTP transport exists in Phase 1.
