//! Conformance of QNC-pro with AGENTS.md. Reads the repo (never writes it) and fails on the
//! first day for what the 2026-11-20 audit found in QNC-AI: apps and orchestrators that decide,
//! blocks that hold others' state, one rule written twice, raw database writes past the owner,
//! several launchers, several logs, several pollers of one table. Usage: `qnc-conformance [ROOT]`.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    process,
};

use serde::Deserialize;

/// The contract every crate under crates/ must have at contracts/modules/<name>.module.json.
#[derive(Debug, Deserialize)]
struct Contract {
    kind: Kind,
    responsibility: String,
    #[serde(default)]
    owns: Vec<String>,
    #[serde(default)]
    writes: Vec<String>,
    #[serde(default)]
    database: Database,
    #[serde(default)]
    polls: f64,
    #[serde(default)]
    launches: bool,
    #[serde(default)]
    paths: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Block,
    Module,
    Panel,
    Form,
    Orchestrator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum Database {
    #[default]
    None,
    Reader,
    Owner,
}

/// Ceilings (AGENTS 4): runtime lines of a crate and of one file, lines of an app's main.
const CRATE_LINES: usize = 600;
const FILE_LINES: usize = 400;
const APP_LINES: usize = 120;
/// A run of this many identical non-trivial lines in two crates is one rule written twice.
const DUPLICATE_RUN: usize = 6;

struct Crate {
    name: String,
    files: Vec<(PathBuf, String)>,
    deps: Vec<String>,
}

fn main() {
    let root = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let mut failures = Vec::new();
    let crates = read_crates(&root.join("crates"));
    let contracts = read_contracts(&root, &crates, &mut failures);
    check_sizes(&crates, &mut failures);
    check_apps(&root.join("apps"), &mut failures);
    check_hierarchy(&crates, &contracts, &mut failures);
    check_database(&crates, &contracts, &mut failures);
    check_one_of_each(&crates, &contracts, &mut failures);
    check_duplicates(&crates, &mut failures);
    check_exception_comments(&crates, &mut failures);
    if failures.is_empty() {
        println!("QNC-pro conformance: ok ({} crates)", crates.len());
        return;
    }
    for failure in &failures {
        println!("  - {failure}");
    }
    println!("QNC-pro conformance: {} failure(s)", failures.len());
    process::exit(1);
}

fn read_crates(dir: &Path) -> Vec<Crate> {
    let mut crates = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return crates };
    for entry in entries.flatten() {
        let dir = entry.path();
        let Ok(manifest) = fs::read_to_string(dir.join("Cargo.toml")) else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        let deps = manifest.lines().filter_map(|line| line.trim().strip_prefix("qnc-")).map(|rest| format!("qnc-{}", rest.split(|c: char| c == ' ' || c == '=').next().unwrap_or(""))).collect();
        let mut files = Vec::new();
        collect_rs(&dir.join("src"), &mut files);
        crates.push(Crate { name, files, deps });
    }
    crates.sort_by(|a, b| a.name.cmp(&b.name));
    crates
}

fn collect_rs(dir: &Path, into: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "tests") {
                continue;
            }
            collect_rs(&path, into);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "tests.rs" || name.ends_with("_tests.rs") {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path) {
                into.push((path, runtime_part(&text)));
            }
        }
    }
}

/// The text before `#[cfg(test)]`: what ships.
fn runtime_part(text: &str) -> String {
    text.split("#[cfg(test)]").next().unwrap_or("").to_string()
}

fn runtime_lines(text: &str) -> usize {
    text.lines().filter(|line| !line.trim().is_empty() && !line.trim().starts_with("//")).count()
}

fn read_contracts(root: &Path, crates: &[Crate], failures: &mut Vec<String>) -> BTreeMap<String, Contract> {
    let mut contracts = BTreeMap::new();
    for krate in crates {
        let file = root.join("contracts").join("modules").join(format!("{}.module.json", krate.name.trim_start_matches("qnc-")));
        match fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|text| serde_json::from_str::<Contract>(&text).map_err(|e| e.to_string())) {
            Ok(contract) => {
                if contract.responsibility.trim().is_empty() || contract.responsibility.len() > 200 || contract.responsibility.matches(". ").count() > 0 {
                    failures.push(format!("C1 {}: responsibility must be one sentence (<= 200 chars)", krate.name));
                }
                contracts.insert(krate.name.clone(), contract);
            }
            Err(error) => failures.push(format!("C1 {}: no valid contract at {} ({error})", krate.name, file.display())),
        }
    }
    contracts
}

fn check_sizes(crates: &[Crate], failures: &mut Vec<String>) {
    for krate in crates {
        let total: usize = krate.files.iter().map(|(_, text)| runtime_lines(text)).sum();
        if total > CRATE_LINES {
            failures.push(format!("C2 {}: {total} runtime lines (ceiling {CRATE_LINES}): split it", krate.name));
        }
        for (path, text) in &krate.files {
            let lines = runtime_lines(text);
            if lines > FILE_LINES {
                failures.push(format!("C2 {}: {lines} runtime lines (ceiling {FILE_LINES})", path.display()));
            }
        }
    }
}

/// An app is only the start of a form: a few lines, no decisions.
fn check_apps(dir: &Path, failures: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let mut files = Vec::new();
        collect_rs(&entry.path().join("src"), &mut files);
        let total: usize = files.iter().map(|(_, text)| runtime_lines(text)).sum();
        let name = entry.file_name().to_string_lossy().to_string();
        if total > APP_LINES {
            failures.push(format!("C3 app {name}: {total} runtime lines (ceiling {APP_LINES}): an app only starts its form"));
        }
        for (path, text) in &files {
            if text.contains(" if ") || text.contains("match ") {
                failures.push(format!("C3 app {}: decides (if/match); the rule belongs to a block", path.display()));
            }
        }
    }
}

/// Block -> block; module -> block/module; panel -> block/module/panel; orchestrator -> block/module; form -> any.
fn check_hierarchy(crates: &[Crate], contracts: &BTreeMap<String, Contract>, failures: &mut Vec<String>) {
    let kinds: HashMap<&str, Kind> = contracts.iter().map(|(name, contract)| (name.as_str(), contract.kind)).collect();
    for krate in crates {
        let Some(&kind) = kinds.get(krate.name.as_str()) else { continue };
        for dep in &krate.deps {
            let Some(&dep_kind) = kinds.get(dep.as_str()) else { continue };
            let allowed = match kind {
                Kind::Block => dep_kind == Kind::Block,
                Kind::Module | Kind::Orchestrator => matches!(dep_kind, Kind::Block | Kind::Module),
                Kind::Panel => matches!(dep_kind, Kind::Block | Kind::Module | Kind::Panel),
                Kind::Form => true,
            };
            if !allowed {
                failures.push(format!("C4 {} ({kind:?}) depends on {dep} ({dep_kind:?}): against the hierarchy block -> module -> panel -> form", krate.name));
            }
        }
        if kind == Kind::Orchestrator {
            for (path, text) in &krate.files {
                let decisions = text.lines().filter(|line| line.contains(" if ") && !line.trim_start().starts_with("//")).count();
                if decisions > 0 {
                    failures.push(format!("C4 {}: {decisions} `if` in an orchestrator: it routes, it does not decide", path.display()));
                }
            }
        }
    }
}

/// Database: rusqlite only with `database` reader/owner; SQL writes only in the owner, and only
/// of the tables its contract names; every table one owner.
fn check_database(crates: &[Crate], contracts: &BTreeMap<String, Contract>, failures: &mut Vec<String>) {
    let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut state_owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for krate in crates {
        let Some(contract) = contracts.get(&krate.name) else { continue };
        for table in &contract.writes {
            owners.entry(table.clone()).or_default().push(krate.name.clone());
        }
        // One state, one owner (AGENTS 3.1): what a contract owns no other contract owns.
        for owned in &contract.owns {
            state_owners.entry(owned.clone()).or_default().push(krate.name.clone());
        }
        for (path, text) in &krate.files {
            if text.contains("rusqlite") && contract.database == Database::None {
                failures.push(format!("C5 {}: uses rusqlite with database: none", path.display()));
            }
            for table in sql_written_tables(text) {
                if contract.database != Database::Owner {
                    failures.push(format!("C5 {}: writes table `{table}` but is not a database owner", path.display()));
                } else if !contract.writes.contains(&table) {
                    failures.push(format!("C5 {}: writes table `{table}` not named in its contract", path.display()));
                }
            }
        }
    }
    for (table, writers) in owners {
        if writers.len() > 1 {
            failures.push(format!("C5 table `{table}` has {} writers: {}", writers.len(), writers.join(", ")));
        }
    }
    for (state, holders) in state_owners {
        if holders.len() > 1 {
            failures.push(format!("C5 state `{state}` is owned by {} blocks: {}; one state, one owner", holders.len(), holders.join(", ")));
        }
    }
}

/// Table names after INSERT INTO / UPDATE / DELETE FROM / CREATE TABLE in string literals.
fn sql_written_tables(text: &str) -> BTreeSet<String> {
    let mut tables = BTreeSet::new();
    for key in ["INSERT INTO ", "INSERT OR REPLACE INTO ", "UPDATE ", "DELETE FROM ", "CREATE TABLE IF NOT EXISTS ", "CREATE TABLE "] {
        for (at, _) in text.match_indices(key) {
            let rest = &text[at + key.len()..];
            let name: String = rest.trim_start_matches(|c: char| c == '"' || c == '\\').chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if !name.is_empty() && name != "IF" {
                tables.insert(name);
            }
        }
    }
    tables
}

/// One launcher, one log, one settings-path builder; pollers only where the contract says so.
fn check_one_of_each(crates: &[Crate], contracts: &BTreeMap<String, Contract>, failures: &mut Vec<String>) {
    let mut launchers = Vec::new();
    let mut loggers = Vec::new();
    let mut path_builders = Vec::new();
    for krate in crates {
        let Some(contract) = contracts.get(&krate.name) else { continue };
        if contract.launches {
            launchers.push(krate.name.clone());
        }
        if contract.paths {
            path_builders.push(krate.name.clone());
        }
        for (path, text) in &krate.files {
            if text.contains("Command::new") && !contract.launches {
                failures.push(format!("C6 {}: Command::new outside the launcher block", path.display()));
            }
            if text.contains("fn log(") {
                loggers.push(krate.name.clone());
            }
            if (text.contains("\"data/") || text.contains(".join(\"data\")") || text.contains("\"catalogs/")) && !contract.paths {
                failures.push(format!("C6 {}: builds a settings/catalog path; only the settings block does", path.display()));
            }
            if (text.contains("Instant::now") || text.contains(".elapsed()")) && contract.polls <= 0.0 {
                failures.push(format!("C7 {}: has a clock but its contract has no `polls`", path.display()));
            }
            if contract.kind != Kind::Panel && contract.kind != Kind::Form && (text.contains("egui::") || text.contains("eframe::")) {
                failures.push(format!("C8 {}: a {:?} draws (egui); drawing is a panel's", path.display(), contract.kind));
            }
        }
    }
    if launchers.len() > 1 {
        failures.push(format!("C6 more than one launcher block: {}", launchers.join(", ")));
    }
    loggers.sort();
    loggers.dedup();
    if loggers.len() > 1 || loggers.iter().any(|name| name != "qnc-diagnostics") {
        failures.push(format!("C6 `fn log(` outside qnc-diagnostics: {}", loggers.join(", ")));
    }
    if path_builders.len() > 1 {
        failures.push(format!("C6 more than one settings-path block: {}", path_builders.join(", ")));
    }
}

/// The same run of lines in two crates is one rule written twice.
fn check_duplicates(crates: &[Crate], failures: &mut Vec<String>) {
    let mut seen: HashMap<Vec<String>, (String, PathBuf)> = HashMap::new();
    let mut reported = BTreeSet::new();
    for krate in crates {
        for (path, text) in &krate.files {
            let lines: Vec<String> = text.lines().map(|line| line.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|line| line.len() >= 24 && !line.starts_with("//") && !line.starts_with("use ")).collect();
            for window in lines.windows(DUPLICATE_RUN) {
                let key = window.to_vec();
                match seen.get(&key) {
                    Some((other, other_path)) if other != &krate.name => {
                        let pair = (other.clone(), krate.name.clone());
                        if reported.insert(pair) {
                            failures.push(format!("C9 {} and {} share {DUPLICATE_RUN}+ identical lines ({} / {}): one rule written twice", other, krate.name, other_path.display(), path.display()));
                        }
                    }
                    Some(_) => {}
                    None => {
                        seen.insert(key, (krate.name.clone(), path.clone()));
                    }
                }
            }
        }
    }
}

/// A rule is in the contract, not in a comment that names the day it was patched in.
fn check_exception_comments(crates: &[Crate], failures: &mut Vec<String>) {
    for krate in crates {
        for (path, text) in &krate.files {
            let count = text.matches("(user 20").count() + text.matches("(live 20").count();
            if count > 0 {
                failures.push(format!("C10 {}: {count} dated exception comment(s); the rule belongs in the contract", path.display()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_tables_are_read_from_sql_literals() {
        let text = r#"conn.execute("INSERT INTO clips (a) VALUES (1)", []); "UPDATE story_items SET x=1"; "CREATE TABLE IF NOT EXISTS marks (id)"; "DELETE FROM \"letters\" WHERE 1""#;
        let tables = sql_written_tables(text);
        assert_eq!(tables.into_iter().collect::<Vec<_>>(), ["clips", "letters", "marks", "story_items"]);
    }

    #[test]
    fn runtime_part_stops_at_the_tests() {
        assert_eq!(runtime_lines(&runtime_part("fn a() {}\n\n// c\n#[cfg(test)]\nmod t { fn b() {} }")), 1);
    }
}
