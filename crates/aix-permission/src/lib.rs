//! Capability resolver boundary. Default deny until Phase 5 implements scoped grants.
/// Deny requests while a host permission resolver has not been configured.
pub fn is_allowed(_capability: &str, _scope: &str) -> bool {
    false
}
#[cfg(test)]
mod tests {
    #[test]
    fn denies_without_resolver() {
        assert!(!super::is_allowed("network.request", "https://example.com"));
    }
}
