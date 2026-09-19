# Resources

Resources are keyed independently of UI nodes. Each has `type` (text, image, audio, video or data), `value`, and optional mimeType. `data` denotes structured JSON data.

Renderers reference resource ids instead of embedding network clients. The host resolves and bounds content, checks permissions and gives the renderer safe handles. Remote URLs cannot be assigned directly to DOM media elements without mediation.

The desktop resolver produces text and structured-data handles. It accepts inline PNG, JPEG, GIF, WebP, MP3, Ogg, WAV, MP4 and WebM data URIs for media. Other media values become `unavailable` handles. Network resource resolution remains disabled until the connector phase routes it through `network.request` authorization.
