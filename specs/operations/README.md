# Operation contract (Phase 2 design)

Each registered operation declares `id`, `input` and `output` JSON Schemas, stable `errors`, `sideEffects`, and `requiredPermissions`. The runtime validates input and output and returns structured success or failure. No operation is executed in Phase 1.

Planned IDs: state.get, state.set, logic.if, collection.filter, collection.sort, data.transform, http.request, time.now, notification.show and ai.generate.

Pure operations operate on JSON. Internal state effects are confined to the current app. Network, filesystem, notifications and AI-provider calls require host-mediated adapters and scope resolution before any effect. Unavailable adapters fail explicitly. Provider credentials are never stored in an App Definition. Arbitrary executable transform strings are prohibited.
