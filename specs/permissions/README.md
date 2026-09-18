# Capability permissions

An app declares requests as `{ "capability": "network.request", "scopes": ["https://api.example.com/weather"] }`. Beta capability names are network.request, file.read, file.write and notification.show. Scopes must be nonempty. Exact matching, URL normalization and canonical directory containment are implemented and tested in Phase 5; no wildcard semantics are promised by the initial schema.

The host separately approves grants. The resolver intersects grants with requests and evaluates actual operation targets at execution time. Unknown capabilities, missing grants and unsupported scope patterns fail closed. File scopes cannot escape through traversal or symlinks. Network redirects and renderer resources must be rechecked. Internal state mutations also pass the resolver with an app-local grant.

Phase 1 provides only a deny-all Rust boundary. It is not a completed permission system.
