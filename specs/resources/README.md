# Resources

Resources are keyed independently of UI nodes. Each has `type` (text, image, audio, video or data), `value`, and optional mimeType. `data` denotes structured JSON data. Phase 1 only validates this envelope; typed content and transport rules arrive with the resource resolver.

Renderers reference resource ids instead of embedding network clients. The host resolves and bounds content, checks permissions and gives the renderer safe handles. Remote URLs cannot be assigned directly to DOM media elements without mediation.
