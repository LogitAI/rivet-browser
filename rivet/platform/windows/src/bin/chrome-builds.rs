//! Finds the rows of Rivet's supported-build table in one browser build, and keeps the signed
//! table. No browser's debug symbols and no other build are read: every value is found in the build
//! at hand by the checked-in descriptions, written from public Chromium source.
//!
//! `chrome-builds find <module> <version> <report.json>` applies every description to one build and
//! writes the values found and not found, by feature.
//! `chrome-builds score <module> <verified-row.json> <score.json>` does the same for a build that
//! has a verified row and compares the two value by value: per feature, hits, wrong values and
//! values not found. It exits 5 when a value it found is wrong.
//! `chrome-builds pending <chrome-stable.json> <chrome-beta.json> <edge.json> <brave.json> <known.json>`
//! prints the builds that have no row yet, most urgent first.
//! `chrome-builds table-key` prints a new signing key pair.
//! `chrome-builds table-add <table.json> <row.json>` puts a row into a table.
//! `chrome-builds table-read <signed-file> <out.json>` writes out a table that verifies under
//! `RIVET_TABLE_PUBLIC_KEY`.
//! `chrome-builds table-merge <table.json> <part.json>...` adds the rows of other tables to a table.
//! `chrome-builds table-sign <table.json> <signed-file> [published-file]` signs a table with the
//! key in `RIVET_TABLE_SIGNING_KEY`, on top of the published rows, which must verify under
//! `RIVET_TABLE_PUBLIC_KEY`.

use std::path::Path;
use std::process::ExitCode;

use rivet_platform_windows::chrome_build::PeFile;
use rivet_platform_windows::{row_find, row_watch, signed_table};

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["find", module, version, report] => find(Path::new(module), version, Path::new(report)),
        ["score", module, verified, out] => {
            return match score(Path::new(module), Path::new(verified), Path::new(out)) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::from(5),
                Err(error) => {
                    eprintln!("chrome-builds: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        ["pending", stable, beta, edge, brave, known] => {
            pending(Path::new(stable), Path::new(beta), Path::new(edge), Path::new(brave), Path::new(known))
        }
        ["gate", results, browser] => gate(Path::new(results), browser, None),
        ["gate", results, browser, row] => gate(Path::new(results), browser, Some(Path::new(row))),
        ["release-sign", version, signed, files @ ..] => release_sign(version, Path::new(signed), files),
        ["release-check", signed, file] => release_check(Path::new(signed), Path::new(file)),
        ["table-key"] => signed_table::generate_key().map(|(private, public)| {
            println!("private {private}");
            println!("public {public}");
        }),
        ["table-add", table, row] => table_add(Path::new(table), Path::new(row)),
        ["table-read", signed, out] => table_read(Path::new(signed), Path::new(out)),
        ["table-merge", table, parts @ ..] => table_merge(Path::new(table), parts),
        ["table-sign", table, signed] => table_sign(Path::new(table), Path::new(signed), None),
        ["table-sign", table, signed, published] => {
            table_sign(Path::new(table), Path::new(signed), Some(Path::new(published)))
        }
        ["table-withdraw", published, signed, builds @ ..] if !builds.is_empty() => {
            table_withdraw(Path::new(published), Path::new(signed), builds)
        }
        _ => Err("usage: chrome-builds find <module> <version> <report.json> | score <module> <verified-row.json> <score.json> | pending <chrome-stable.json> <chrome-beta.json> <edge.json> <brave.json> <known.json> | gate <results.json> <browser> [row.json] | release-sign <version> <signed-file> <file>... | release-check <signed-file> <file> | table-key | table-add <table.json> <row.json> | table-read <signed-file> <out.json> | table-merge <table.json> <part.json>... | table-sign <table.json> <signed-file> [published-file] | table-withdraw <published-file> <signed-file> <browser@version>...".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("chrome-builds: {error}");
            ExitCode::FAILURE
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

/// Applies every description to one module.
fn find_in(module: &Path) -> Result<row_find::Found, String> {
    let descriptions = row_find::descriptions()?;
    let bytes = read(module)?;
    let pe = PeFile::parse(&bytes).ok_or("the module is not a 64-bit PE image")?;
    Ok(row_find::find(&pe, &descriptions))
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())? + "
";
    std::fs::write(path, text).map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// Finds the values of one build and reports them by feature. Not finding a value is a result,
/// not a failure.
fn find(module: &Path, version: &str, report: &Path) -> Result<(), String> {
    let found = find_in(module)?;
    let groups = row_find::report(&found);
    let values: serde_json::Map<String, serde_json::Value> = found
        .row_values()
        .map(|(path, value)| (path.clone(), format!("{value:#x}").into()))
        .collect();
    write_json(
        report,
        &serde_json::json!({
            "module": module.display().to_string(),
            "version": version,
            "values": values,
            "features": groups,
        }),
    )?;
    println!("{version}:");
    for (group, each) in &groups {
        println!("  {group}: {} found, {} not found", each.found.len(), each.not_found.len());
        for line in &each.not_found {
            println!("    not found: {line}");
        }
    }
    Ok(())
}

/// Finds the values of a build that has a verified row and scores them against it. True when no
/// value found is wrong.
fn score(module: &Path, verified: &Path, out: &Path) -> Result<bool, String> {
    let verified = json_file(verified)?;
    let found = find_in(module)?;
    let score = row_find::score(&found, &verified);
    write_json(
        out,
        &serde_json::json!({
            "browser": verified["frame"]["browser"],
            "version": verified["frame"]["version"],
            "features": score.groups,
        }),
    )?;
    println!(
        "{} {}:",
        verified["frame"]["browser"].as_str().unwrap_or("?"),
        verified["frame"]["version"].as_str().unwrap_or("?")
    );
    for (group, each) in &score.groups {
        println!(
            "  {group}: {} hit, {} wrong, {} not found, {} unchecked",
            each.hit,
            each.wrong.len(),
            each.not_found.len(),
            each.unchecked.len()
        );
        for line in &each.wrong {
            println!("    wrong: {line}");
        }
    }
    Ok(score.wrong() == 0)
}

fn json_file(path: &Path) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&read(path)?)
        .map_err(|error| format!("{} is not JSON: {error}", path.display()))
}

fn pending(
    stable: &Path,
    beta: &Path,
    edge: &Path,
    brave: &Path,
    known: &Path,
) -> Result<(), String> {
    let mut candidates = row_watch::chrome_candidates(&json_file(stable)?, &json_file(beta)?);
    candidates.extend(row_watch::edge_candidates(&json_file(edge)?));
    candidates.extend(row_watch::brave_candidates(&json_file(brave)?));
    let known = json_file(known)?
        .as_array()
        .ok_or("the known builds are not a list")?
        .iter()
        .filter_map(|pair| Some((pair[0].as_str()?.to_owned(), pair[1].as_str()?.to_owned())))
        .collect();
    let left = row_watch::pending(candidates, &known);
    println!(
        "{}",
        serde_json::to_string_pretty(&left).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn table_add(table_path: &Path, row: &Path) -> Result<(), String> {
    let row = json_file(row)?;
    let mut table = json_file(table_path)?;
    signed_table::upsert_row(&mut table, row)?;
    let text = serde_json::to_string_pretty(&table).map_err(|error| error.to_string())? + "\n";
    std::fs::write(table_path, text).map_err(|error| error.to_string())
}

fn table_read(signed: &Path, out: &Path) -> Result<(), String> {
    let public =
        std::env::var("RIVET_TABLE_PUBLIC_KEY").map_err(|_| "RIVET_TABLE_PUBLIC_KEY is not set")?;
    let file = read(signed)?;
    let table = signed_table::parse(signed_table::open(&public, &file)?)?;
    let text = serde_json::to_string(&table).map_err(|error| error.to_string())?;
    std::fs::write(out, text).map_err(|error| error.to_string())
}

fn table_merge(table_path: &Path, parts: &[&str]) -> Result<(), String> {
    let mut table = json_file(table_path)?;
    for part in parts {
        // A sequence of zero: only signing sets the real one.
        table = signed_table::merge_for_signing(&json_file(Path::new(part))?, Some(&table), 0)?;
        table["sequence"] = 0.into();
    }
    let text = serde_json::to_string_pretty(&table).map_err(|error| error.to_string())? + "\n";
    std::fs::write(table_path, text).map_err(|error| error.to_string())
}

fn table_sign(table: &Path, signed: &Path, published: Option<&Path>) -> Result<(), String> {
    let private = std::env::var("RIVET_TABLE_SIGNING_KEY")
        .map_err(|_| "RIVET_TABLE_SIGNING_KEY is not set")?;
    let published = match published {
        Some(path) => {
            let public = std::env::var("RIVET_TABLE_PUBLIC_KEY")
                .map_err(|_| "RIVET_TABLE_PUBLIC_KEY is not set")?;
            let file = read(path)?;
            Some(signed_table::parse(signed_table::open(&public, &file)?)?)
        }
        None => None,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let merged = signed_table::merge_for_signing(&json_file(table)?, published.as_ref(), now)?;
    let payload = serde_json::to_vec(&merged).map_err(|error| error.to_string())?;
    let file = signed_table::sign(&private, &payload)?;
    std::fs::write(signed, file).map_err(|error| error.to_string())?;
    println!(
        "sequence {}, {} rows",
        merged["sequence"],
        merged["rows"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}

/// Signs the published table again without the rows of `builds`, each `browser@version`.
fn table_withdraw(published: &Path, signed: &Path, builds: &[&str]) -> Result<(), String> {
    let private = std::env::var("RIVET_TABLE_SIGNING_KEY")
        .map_err(|_| "RIVET_TABLE_SIGNING_KEY is not set")?;
    let public =
        std::env::var("RIVET_TABLE_PUBLIC_KEY").map_err(|_| "RIVET_TABLE_PUBLIC_KEY is not set")?;
    let table = signed_table::parse(signed_table::open(&public, &read(published)?)?)?;
    let pairs = builds
        .iter()
        .map(|build| {
            build
                .split_once('@')
                .ok_or(format!("{build} is not browser@version"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let table = signed_table::withdraw_rows(&table, &pairs, now)?;
    let payload = serde_json::to_vec(&table).map_err(|error| error.to_string())?;
    std::fs::write(signed, signed_table::sign(&private, &payload)?)
        .map_err(|error| error.to_string())?;
    println!(
        "sequence {}, {} rows",
        table["sequence"],
        table["rows"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}

/// Refuses a browser that did not pass every feature check, so it is never listed as supported.
/// The row is the one being published: a check of a feature it turns off is not required.
fn gate(results: &Path, browser: &str, row: Option<&Path>) -> Result<(), String> {
    let row = match row {
        Some(path) => json_file(path)?,
        None => serde_json::Value::Null,
    };
    let text = String::from_utf8(read(results)?)
        .map_err(|error| format!("the feature results are not text: {error}"))?;
    // PowerShell writes UTF-8 with a byte order mark.
    let results: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|error| format!("the feature results are not JSON: {error}"))?;
    let notes = row_watch::publish_gate_for_row(&results, browser, &row)
        .map_err(|reasons| format!("{browser} is not supported: {}", reasons.join("; ")))?;
    for note in notes {
        println!("note: {note}");
    }
    Ok(())
}

/// Signs the SHA-256 of each release file, so an installer or update applies only these files.
fn release_sign(version: &str, signed: &Path, files: &[&str]) -> Result<(), String> {
    let private = std::env::var("RIVET_TABLE_SIGNING_KEY")
        .map_err(|_| "RIVET_TABLE_SIGNING_KEY is not set")?;
    let mut read_files = Vec::new();
    for file in files {
        let path = Path::new(file);
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("{file} has no file name"))?
            .to_owned();
        read_files.push((name, read(path)?));
    }
    let listed: Vec<(&str, &[u8])> = read_files
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    let manifest = signed_table::artifact_manifest(version, &listed)?;
    let file = signed_table::sign(&private, &manifest)?;
    std::fs::write(signed, file).map_err(|error| error.to_string())
}

/// Fails unless a file matches its hash in a release manifest signed with the release key.
fn release_check(signed: &Path, file: &Path) -> Result<(), String> {
    let public =
        std::env::var("RIVET_TABLE_PUBLIC_KEY").map_err(|_| "RIVET_TABLE_PUBLIC_KEY is not set")?;
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the file has no name")?;
    let version = signed_table::check_artifact(&public, &read(signed)?, name, &read(file)?)?;
    println!("{name} matches release {version}");
    Ok(())
}
