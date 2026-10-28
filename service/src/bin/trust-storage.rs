//! Local operator utility. Keys are file inputs, never arguments or stdout.
use std::{path::Path, process::ExitCode};
use trust_security::DatabaseKey;

fn main() -> ExitCode {
    if trust_security::disable_process_dumps().is_err() {
        eprintln!("Cannot disable process dumps; refusing to load keys.");
        return ExitCode::FAILURE;
    }
    if run().is_err() {
        eprintln!("Protected storage operation failed. Check private paths, key files, format and that Trust is stopped. Existing destinations are never overwritten.");
        return ExitCode::FAILURE;
    }
    println!("Protected storage operation completed. Preserve keys separately and verify recovery before switching files.");
    ExitCode::SUCCESS
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.first().and_then(|value| value.to_str()) {
        Some("keygen") if args.len() == 2 => {
            let path = args.get(1).ok_or("Missing key path")?;
            DatabaseKey::generate()?.write_new(Path::new(path))?;
        }
        Some("encrypt-copy") if matches!(args.len(), 4 | 5) => {
            let source = Path::new(args.get(1).ok_or("Missing source")?);
            let destination = Path::new(args.get(2).ok_or("Missing destination")?);
            let key = DatabaseKey::load(Path::new(
                args.get(3).ok_or("Missing destination key file")?,
            ))?;
            let old_key = args
                .get(4)
                .map(|value| DatabaseKey::load(Path::new(value)))
                .transpose()?;
            trust_service::storage::encrypted_copy(source, destination, &key, old_key.as_ref())?;
        }
        _ => {
            eprintln!("Usage: trust-storage keygen KEY_FILE\n       trust-storage encrypt-copy SOURCE NEW_DESTINATION DESTINATION_KEY_FILE [SOURCE_KEY_FILE]");
            return Err("Invalid storage command".into());
        }
    }
    Ok(())
}
