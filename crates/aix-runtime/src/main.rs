//! Minimal executable for checking that the Rust workspace is runnable.
fn main() {
    println!(
        "AIX Runtime skeleton — App Spec {}",
        aix_runtime::spec_version()
    );
}
