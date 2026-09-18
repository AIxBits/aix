//! Runtime boundary. Operation execution is introduced in Phase 2.
/// Current supported specification version.
pub fn spec_version() -> &'static str {
    aix_core::SPEC_VERSION
}
