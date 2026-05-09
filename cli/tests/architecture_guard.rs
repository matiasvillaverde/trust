use std::fs;
use std::path::{Path, PathBuf};

const CLI_SRC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src");

#[test]
fn cli_risk_mutations_route_through_trust_facade() {
    let mut violations = Vec::new();
    for file in rust_source_files(Path::new(CLI_SRC)) {
        if file.ends_with("main.rs")
            || file
                .components()
                .any(|component| component.as_os_str() == "commands")
        {
            continue;
        }

        let source = production_source(&file);
        let lines: Vec<&str> = source.lines().collect();
        for (line_number, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let previous = line_number
                .checked_sub(1)
                .and_then(|index| lines.get(index))
                .map(|line| line.trim())
                .unwrap_or_default();
            let previous_previous = line_number
                .checked_sub(2)
                .and_then(|index| lines.get(index))
                .map(|line| line.trim())
                .unwrap_or_default();
            let context = format!("{previous_previous} {previous} {trimmed}");
            let routed_through_trust = context.contains("trust.")
                || context.contains(".trust")
                || (trimmed.starts_with('.') && matches!(previous, "trust" | ".trust"));
            for needle in [
                ".create_trade(",
                ".fund_trade(",
                ".submit_trade(",
                ".cancel_trade(",
                ".sync_trade(",
                ".close_trade(",
                ".modify_stop(",
                ".modify_target(",
                ".create_transaction(",
                ".create_rule(",
            ] {
                if trimmed.contains(needle) && !routed_through_trust && !trimmed.contains("self.") {
                    violations.push(format!(
                        "{}:{}: `{needle}` is not routed through TrustFacade",
                        display_path(&file),
                        line_number + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "CLI risk mutation boundary violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn cli_direct_broker_calls_are_read_only_or_credential_setup_only() {
    let allowed = [
        "AlpacaBroker::setup_keys",
        "AlpacaBroker::read_keys",
        "AlpacaBroker::delete_keys",
        "AlpacaBroker::fetch_asset_metadata",
        "IbkrBroker::setup_connection",
        "IbkrBroker::read_connection",
        "IbkrBroker::delete_connection",
        "IbkrBroker::fetch_contract_metadata_for_category",
    ];

    let mut violations = Vec::new();
    for file in rust_source_files(Path::new(CLI_SRC)) {
        let source = production_source(&file);
        for (line_number, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            for needle in ["AlpacaBroker::", "IbkrBroker::"] {
                if trimmed.contains(needle) && !allowed.iter().any(|item| trimmed.contains(item)) {
                    violations.push(format!(
                        "{}:{}: direct broker call is outside the allowed read-only/credential surface: {trimmed}",
                        display_path(&file),
                        line_number + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "CLI direct broker boundary violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn cli_constructs_sqlite_only_at_dispatcher_boundaries() {
    let mut violations = Vec::new();
    for file in rust_source_files(Path::new(CLI_SRC)) {
        let source = production_source(&file);
        for (line_number, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.contains("SqliteDatabase::") && !file.ends_with("dispatcher.rs") {
                violations.push(format!(
                    "{}:{}: SQLite construction must stay at dispatcher/database import-export boundaries",
                    display_path(&file),
                    line_number + 1
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "CLI direct SQLite boundary violations:\n{}",
        violations.join("\n")
    );
}

fn rust_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_source_files(root, &mut files);
    files.sort();
    files
}

fn collect_rust_source_files(path: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display());
    });
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read directory entry: {error}"));
        let path = entry.path();
        if path.is_dir() {
            collect_rust_source_files(&path, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn production_source(path: &Path) -> String {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    strip_cfg_test_modules(&source)
}

fn strip_cfg_test_modules(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if trimmed.starts_with("#[cfg(test)]") {
            if let Some(next_line) = lines.get(index + 1) {
                if next_line.trim_start().starts_with("mod tests") {
                    index = skip_braced_item(&lines, index + 1);
                    continue;
                }
            }
        }

        output.push(lines[index]);
        index += 1;
    }

    output.join("\n")
}

fn skip_braced_item(lines: &[&str], start: usize) -> usize {
    let mut depth = 0_i32;
    let mut seen_open = false;
    let mut index = start;

    while index < lines.len() {
        for ch in lines[index].chars() {
            match ch {
                '{' => {
                    depth += 1;
                    seen_open = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        index += 1;
        if seen_open && depth <= 0 {
            break;
        }
    }

    index
}

fn display_path(path: &Path) -> String {
    path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .unwrap_or(path)
        .display()
        .to_string()
}
