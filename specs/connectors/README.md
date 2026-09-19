# Connectors

HTTP connector declarations turn external REST endpoints into registered AIX Operations. Each connector has an `id`, type `http`, an HTTP(S) `baseUrl`, and operations with a unique namespaced `id`, fixed method, and path template. Base URLs cannot contain credentials, a query, or a fragment. A declaration maps data to a request; it never grants network authority.

```json
{
  "id": "weather",
  "type": "http",
  "baseUrl": "https://api.example.com/v1",
  "operations": [
    { "id": "weather.get", "method": "GET", "path": "/weather/{city}" }
  ]
}
```

Connector Operation input is provider-neutral:

```json
{
  "pathParams": { "city": "New York" },
  "query": { "units": "metric", "tag": ["current", "public"] },
  "headers": { "accept": "application/json" },
  "body": null
}
```

Path values are percent encoded as one segment. Query values can be strings, numbers, booleans, or arrays of those scalar types. Header values must be strings. The method and destination base are fixed by the connector declaration. Output has `status`, normalized string `headers`, and `body`; JSON media types become JSON values, other UTF-8 responses become strings, and an empty body becomes `null`. HTTP error statuses are returned as responses so workflows can inspect them.

The default adapter uses a 10 second whole-request timeout, a 256 KiB request-body limit, a 1 MiB response-body limit, at most 64 headers/16 KiB of header data, an 8 KiB URL limit, and five redirects. It rejects host-controlled framing headers. Redirects are handled manually: every destination is checked by the permission resolver before sending, and authorization/cookie headers are removed when the origin changes. Timeout, transport, decoding, size, malformed redirect, and permission failures use stable Operation error codes.

## OpenAPI import

The Beta importer accepts local JSON OpenAPI 3.0 or 3.1 documents up to 1 MiB, with exactly one top-level HTTP(S) server and `paths` containing GET, POST, PUT, PATCH, or DELETE operations. It emits at most 256 Operations. Every imported operation needs a unique `operationId` in lowercase `namespace.name` form. Path, query, and header parameters use the generic connector input shown above.

The importer rejects `$ref`, server variables, per-operation servers, callbacks, and security requirements. It does not retrieve remote documents, infer credentials, execute examples, or import provider code. Convert a local document with:

```sh
cargo run -p aix-runtime -- import-openapi openapi.json weather
```

The command prints a connector declaration for review and insertion into an App Definition.
