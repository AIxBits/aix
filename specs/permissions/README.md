# Capability permissions

An app declares requests as `{ "capability": "network.request", "scopes": ["https://api.example.com/weather"] }`. Beta capability names are ai.generate, network.request, file.read, file.write and notification.show. Scopes must be nonempty. Exact matching, URL normalization and canonical directory containment are implemented and tested in Phase 5; no wildcard semantics are promised by the initial schema. The `ai.generate` capability authorizes use of a user-selected provider profile without granting the app arbitrary HTTP access.

The host separately approves grants. The resolver intersects grants with requests and evaluates actual operation targets at execution time. Unknown capabilities, missing grants and unsupported scope patterns fail closed. File scopes cannot escape through traversal or symlinks. Network redirects and renderer resources must be rechecked. Internal state mutations also pass the resolver with an app-local grant.

Phase 2 operation definitions advertise required capabilities, and app validation rejects workflows that fail to request them. Requested capabilities still confer no authority. The existing Rust permission boundary denies all host access until Phase 5 implements scoped grants and resolution.
