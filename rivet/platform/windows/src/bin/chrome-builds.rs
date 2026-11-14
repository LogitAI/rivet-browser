//! Makes and publishes rows of Rivet's supported-build table. No browser's debug symbols are read:
//! a new row comes from the verified prior row, its fingerprints, the prior module and the new one.
//!
//! `chrome-builds find <prior-row.json> <prior-module> <new-module> <version> <row.json> <report.json>`
//! makes a candidate row for a new build and exits 3 when the row needs a person's review.
//! `chrome-builds score <prior-row.json> <prior-module> <new-module> <verified-row.json> <score.json>`
//! makes the row of a build that already has a verified row the same way, compares the two value
//! by value and writes the score of each feature. It exits 5 when a value it found is wrong.
//! `chrome-builds fingerprint <row.json> <module> <row-out.json>` adds the fingerprints of a row's
//! functions, read from its own module, so the next build's row can start from them.
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

use rivet_platform_windows::chrome_build::{self, PeFile};
use rivet_platform_windows::row_port::{self, PortReport};
use rivet_platform_windows::{row_watch, signed_table};

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["find", prior, prior_module, module, version, row, report] => {
            return match find(
                Path::new(prior),
                Path::new(prior_module),
                Path::new(module),
                version,
                Path::new(row),
                Path::new(report),
            ) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::from(3),
                Err(error) => {
                    eprintln!("chrome-builds: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        ["score", prior, prior_module, module, verified, out] => {
            return match score(
                Path::new(prior),
                Path::new(prior_module),
                Path::new(module),
                Path::new(verified),
                Path::new(out),
            ) {
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
        ["fingerprint", row, module, out] => fingerprint(Path::new(row), Path::new(module), Path::new(out)),
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
        _ => Err("usage: chrome-builds find <prior-row.json> <prior-module> <new-module> <version> <row.json> <report.json> | score <prior-row.json> <prior-module> <new-module> <verified-row.json> <score.json> | pending <chrome-stable.json> <chrome-beta.json> <edge.json> <brave.json> <known.json> | gate <results.json> <browser> [row.json] | fingerprint <row.json> <module> <row-out.json> | release-sign <version> <signed-file> <file>... | release-check <signed-file> <file> | table-key | table-add <table.json> <row.json> | table-read <signed-file> <out.json> | table-merge <table.json> <part.json>... | table-sign <table.json> <signed-file> [published-file] | table-withdraw <published-file> <signed-file> <browser@version>...".into()),
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

/// Makes the row of `version` from the prior row, the prior row's own module and the new module.
fn make(
    prior: &serde_json::Value,
    prior_module: &Path,
    module: &Path,
    version: &str,
) -> Result<(serde_json::Value, PortReport), String> {
    let prior_bytes = read(prior_module)?;
    let prior_pe =
        PeFile::parse(&prior_bytes).ok_or("the prior module is not a 64-bit PE image")?;
    // A module of another build would give fingerprints and class names of the wrong functions.
    let start = prior["frame"]["hwnd_for_view"]
        .as_u64()
        .and_then(|address| u32::try_from(address).ok());
    let prologue = prior["frame"]["hwnd_for_view_prologue"]
        .as_str()
        .and_then(chrome_build::hex);
    match (start, prologue) {
        (Some(start), Some(prologue))
            if prior_pe.read(start, prologue.len()) == Some(prologue.as_slice()) => {}
        _ => return Err("the prior module is not the build of the prior row".into()),
    }
    // The fingerprints the prior row stored were read from this module when the row was made.
    // A row stored without them gets them from the module now.
    let prints = match prior["fingerprints"].as_object() {
        Some(stored) => stored.clone(),
        None => {
            let (prints, missing) = row_port::fingerprints_of(prior, &prior_pe);
            if !missing.is_empty() {
                println!(
                    "the prior build gives no fingerprint for {}",
                    missing.join(", ")
                );
            }
            prints
        }
    };
    // Each class table of the prior row is found again by its views class's name too.
    let classes = row_port::prior_classes(prior, &prior_pe);
    let bytes = read(module)?;
    let pe = PeFile::parse(&bytes).ok_or("the new module is not a 64-bit PE image")?;
    let (mut row, mut report) =
        row_port::find_candidate(prior, &prints, Some(&prior_pe), &classes, version, &pe);
    row_port::leave_out_screen_origin(&mut row, &mut report);
    report.review.extend(row_port::hand_only_review(prior));
    println!(
        "exception tables: {} function starts in the prior build, {} in the new one; {} class tables named",
        prior_pe.function_starts().len(),
        pe.function_starts().len(),
        classes.len()
    );
    Ok((row, report))
}

/// Makes a candidate row for a new build. True when nothing needs a person's review.
fn find(
    prior_path: &Path,
    prior_module: &Path,
    module: &Path,
    version: &str,
    row_path: &Path,
    report_path: &Path,
) -> Result<bool, String> {
    let prior = json_file(prior_path)?;
    let (row, report) = make(&prior, prior_module, module, version)?;
    let text = |value: &dyn erased::Json| value.pretty();
    std::fs::write(row_path, text(&row)).map_err(|error| error.to_string())?;
    std::fs::write(report_path, text(&report)).map_err(|error| error.to_string())?;
    println!(
        "{} {} from {}: {} of {} addresses found by fingerprint, {} settled by two methods, {} values read, {} carried, {} features off, {} for review",
        report.browser,
        report.version,
        report.prior_version,
        report.fingerprinted.len(),
        report.addresses,
        report.settled.len(),
        report.derived.len(),
        report.carried.len(),
        report.switched_off.len(),
        report.review.len()
    );
    Ok(report.review.is_empty())
}

/// Makes the row of a build that already has a verified row, and scores it against that row.
/// True when no value it found is wrong. Nothing is written but the score.
fn score(
    prior_path: &Path,
    prior_module: &Path,
    module: &Path,
    verified_path: &Path,
    out: &Path,
) -> Result<bool, String> {
    let prior = json_file(prior_path)?;
    let verified = json_file(verified_path)?;
    let version = verified["frame"]["version"]
        .as_str()
        .ok_or("the verified row has no frame version")?
        .to_owned();
    let (row, report) = make(&prior, prior_module, module, &version)?;
    let score = row_port::score_row(&row, &report, &verified);
    std::fs::write(
        out,
        serde_json::to_string_pretty(&score).map_err(|error| error.to_string())? + "\n",
    )
    .map_err(|error| error.to_string())?;
    println!(
        "{} {} from {}:",
        score.browser, score.version, score.prior_version
    );
    for (feature, each) in &score.features {
        let state = if each.off { "off" } else { "on" };
        println!(
            "  {feature}: {state}, {} equal, {} different, {} not held",
            each.equal,
            each.different.len(),
            each.not_held.len()
        );
        for line in &each.different {
            println!("    wrong: {line}");
        }
    }
    if !score.review.is_empty() {
        println!(
            "  the row would stop for review: {}",
            score.review.join("; ")
        );
    }
    Ok(score.wrong() == 0)
}

mod erased {
    pub trait Json {
        fn pretty(&self) -> String;
    }
    impl<T: serde::Serialize> Json for T {
        fn pretty(&self) -> String {
            serde_json::to_string_pretty(self).unwrap_or_default() + "\n"
        }
    }
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

/// Adds the function fingerprints of a verified row, read from its own module, so a runtime can
/// find the same functions in a build that has no row yet.
fn fingerprint(row_path: &Path, module: &Path, out: &Path) -> Result<(), String> {
    let mut row: serde_json::Value = serde_json::from_slice(&read(row_path)?)
        .map_err(|error| format!("the row is not JSON: {error}"))?;
    let bytes = read(module)?;
    let pe = PeFile::parse(&bytes).ok_or("the module is not a 64-bit PE image")?;
    row_port::add_fingerprints(&mut row, &pe)?;
    let text = serde_json::to_string_pretty(&row).map_err(|error| error.to_string())?;
    std::fs::write(out, text).map_err(|error| format!("could not write {}: {error}", out.display()))
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
