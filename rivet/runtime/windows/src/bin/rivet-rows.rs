//! What the row generator needs from the runtime's own idea of a row.
//!
//! `rivet-rows row <browser> <version>` prints a row built into this binary as `{frame, build}`.
//! `rivet-rows prior <browser> <version>` prints the newest row at or below a version, in this
//! binary or in the tables given with `--table <file>`.
//! `rivet-rows known` prints the browser and version of each of those rows.
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

/// The rows of this binary and of the tables named by `--table <file>`.
#[cfg(windows)]
fn rows_with(arguments: &[&str]) -> Result<Vec<serde_json::Value>, String> {
    let mut rows = rivet_runtime_windows::compiled_rows();
    for pair in arguments.chunks(2) {
        let ["--table", path] = pair else {
            return Err(format!("unknown argument {pair:?}"));
        };
        rows.extend(table_rows(path)?["rows"].as_array().cloned().unwrap_or_default());
    }
    Ok(rows)
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
        ["prior", browser, version, rest @ ..] => {
            // The newest row at or below a version, so a beta row of the next milestone serves that
            // milestone's stable build, and a later row never serves an older build.
            let parts: Vec<u64> = version.split('.').filter_map(|part| part.parse().ok()).collect();
            let row = rows_with(rest)?
                .into_iter()
                .filter(|row| {
                    row["build"]["browser"] == *browser && !row["frame"].is_null() && version_of(row) <= parts
                })
                .max_by_key(version_of)
                .ok_or_else(|| format!("no row for {browser} at or below {version}"))?;
            print(&row);
            Ok(())
        }
        ["known", rest @ ..] => {
            let known: Vec<_> = rows_with(rest)?
                .iter()
                .map(|row| {
                    let version: Vec<String> = version_of(row).iter().map(u64::to_string).collect();
                    serde_json::json!([row["build"]["browser"], version.join(".")])
                })
                .collect();
            print(&serde_json::Value::Array(known));
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
        _ => Err("usage: rivet-rows row <browser> <version> | prior <browser> <version> [--table <file>] | known [--table <file>] | check <table.json>".into()),
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
