//! What the row generator needs from the runtime's own idea of a row.
//!
//! `rivet-rows row <browser> <version>` prints a row built into this binary as `{frame, build}`.
//! `rivet-rows latest <browser>` prints the same for that browser's newest row, in this binary or in
//! a table given with `--table <file>`.
//! `rivet-rows check <table.json>` fails when this runtime cannot read a row of a table.

#[cfg(windows)]
fn version_of(row: &serde_json::Value) -> Vec<u64> {
    row["build"]["version"].as_array().map_or(vec![], |parts| {
        parts.iter().filter_map(|part| part.as_u64()).collect()
    })
}

#[cfg(windows)]
fn table_rows(path: &str) -> Result<serde_json::Value, String> {
    let text = std::fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    serde_json::from_slice(&text).map_err(|error| format!("{path}: {error}"))
}

#[cfg(windows)]
fn run(arguments: &[String]) -> Result<(), String> {
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let print = |row: &serde_json::Value| {
        println!("{}", serde_json::to_string_pretty(row).unwrap_or_default());
    };
    match arguments.as_slice() {
        ["row", browser, version] => {
            let parts: Vec<u64> = version.split('.').filter_map(|part| part.parse().ok()).collect();
            let row = rivet_runtime_windows::compiled_rows()
                .into_iter()
                .find(|row| row["build"]["browser"] == *browser && version_of(row) == parts)
                .ok_or_else(|| format!("no row for {browser} {version}"))?;
            print(&row);
            Ok(())
        }
        ["latest", browser, rest @ ..] => {
            let mut rows = rivet_runtime_windows::compiled_rows();
            if let ["--table", path] = rest {
                rows.extend(table_rows(path)?["rows"].as_array().cloned().unwrap_or_default());
            }
            let row = rows
                .into_iter()
                .filter(|row| row["build"]["browser"] == *browser && !row["frame"].is_null())
                .max_by_key(version_of)
                .ok_or_else(|| format!("no row for {browser}"))?;
            print(&row);
            Ok(())
        }
        ["check", path] => {
            let refused = rivet_runtime_windows::refused_rows(&table_rows(path)?);
            if refused.is_empty() {
                Ok(())
            } else {
                Err(format!("this runtime cannot read {refused:?}"))
            }
        }
        _ => Err("usage: rivet-rows row <browser> <version> | latest <browser> [--table <file>] | check <table.json>".into()),
    }
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rivet-rows: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("rivet-rows runs on Windows");
}
