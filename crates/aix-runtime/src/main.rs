//! Minimal command-line entry point for inspecting and validating runtime data.

use std::error::Error;
use std::fs;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let runtime = aix_runtime::Runtime::new()?;
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        None => {
            println!(
                "AIX Runtime — App Spec {}, {} operations registered",
                aix_runtime::spec_version(),
                runtime.operations().definitions().len()
            );
        }
        Some("operations") if arguments.next().is_none() => {
            println!(
                "{}",
                serde_json::to_string_pretty(&runtime.operations().definitions())?
            );
        }
        Some("validate") => {
            let path = arguments
                .next()
                .ok_or("usage: aix-runtime validate <app.aix.json>")?;
            if arguments.next().is_some() {
                return Err("usage: aix-runtime validate <app.aix.json>".into());
            }
            let source = fs::read_to_string(&path)?;
            let app = runtime.load_json(&source)?;
            println!("Valid AIX App: {} ({})", app.metadata.name, path);
        }
        Some("import-openapi") => {
            let path = arguments
                .next()
                .ok_or("usage: aix-runtime import-openapi <openapi.json> <connector-id>")?;
            let connector_id = arguments
                .next()
                .ok_or("usage: aix-runtime import-openapi <openapi.json> <connector-id>")?;
            if arguments.next().is_some() {
                return Err(
                    "usage: aix-runtime import-openapi <openapi.json> <connector-id>".into(),
                );
            }
            let source = fs::read_to_string(path)?;
            let connector = aix_runtime::import_openapi_json(&source, &connector_id)?;
            println!("{}", serde_json::to_string_pretty(&connector)?);
        }
        _ => {
            return Err("usage: aix-runtime [operations|validate <app.aix.json>|import-openapi <openapi.json> <connector-id>]".into());
        }
    }
    Ok(())
}
