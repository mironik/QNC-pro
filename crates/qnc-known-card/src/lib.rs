//! A card put in that a project of this computer already has: the originals on the card now
//! are looked for among the clips of every project the registry names, newest first, by the
//! card's source and each original's URI. The newest project with any of them is the card's:
//! how many of the card's originals it has, which originals it has not (new), and how many it
//! has only Selected, not imported (its ingest was stopped before the end). None: the card is
//! new, or formatted and shot again. Read only; a project whose database cannot be read is
//! said in the log and passed over.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use qnc_db::Reader;

/// The newest project that has clips of a card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    pub project_id: String,
    pub name: String,
    pub folder: PathBuf,
    /// How many of the card's originals the project has.
    pub had: usize,
    /// The originals on the card the project has not (QNC URIs, in the card's order).
    pub new: Vec<String>,
    /// How many of the card's originals the project has only Selected, not imported.
    pub unfinished: usize,
}

/// A project of the registry: id, name, folder, newest first.
fn projects(root: &Path) -> Result<Vec<(String, String, PathBuf)>, String> {
    let reader = qnc_registry::reader(root)?;
    let mut statement = reader
        .connection()
        .prepare("SELECT p.project_id, p.name, l.local_path FROM projects p JOIN project_storage_locations l ON l.project_id = p.project_id ORDER BY p.created_at DESC, p.project_id DESC")
        .map_err(|e| e.to_string())?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, PathBuf::from(r.get::<_, String>(2)?)))).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// The newest project that has clips of the card `source_uri` whose originals are among
/// `card` (the originals on it now); `None` when no project has any of them.
pub fn find(root: &Path, source_uri: &str, card: &[String]) -> Result<Option<Known>, String> {
    for (project_id, name, folder) in projects(root)? {
        let db = folder.join(qnc_project_make::PROJECT_DB);
        if !db.is_file() {
            continue;
        }
        let originals = match Reader::open(&db).and_then(|reader| qnc_catalog::originals(&reader)) {
            Ok(originals) => originals,
            Err(error) => {
                qnc_diagnostics::log(format!("known card: project {project_id} not read: {error}"));
                continue;
            }
        };
        let had: BTreeSet<&str> = originals.iter().filter(|o| o.source_uri == source_uri).map(|o| o.original_uri.as_str()).collect();
        let imported: BTreeSet<&str> = originals.iter().filter(|o| o.source_uri == source_uri && o.imported).map(|o| o.original_uri.as_str()).collect();
        let common = card.iter().filter(|original| had.contains(original.as_str())).count();
        if common == 0 {
            continue;
        }
        let new = card.iter().filter(|original| !had.contains(original.as_str())).cloned().collect();
        let unfinished = card.iter().filter(|original| had.contains(original.as_str()) && !imported.contains(original.as_str())).count();
        return Ok(Some(Known { project_id, name, folder, had: common, new, unfinished }));
    }
    Ok(None)
}

/// The originals of the card `source_uri` the project at `folder` already has (what a new
/// project of only the new clips leaves out).
pub fn originals_of(folder: &Path, source_uri: &str) -> Result<BTreeSet<String>, String> {
    let reader = Reader::open(&folder.join(qnc_project_make::PROJECT_DB))?;
    Ok(qnc_catalog::originals(&reader)?.into_iter().filter(|o| o.source_uri == source_uri).map(|o| o.original_uri).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registry_without_projects_knows_no_card() {
        let root = std::env::temp_dir().join(format!("qnc-known-card-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("data")).unwrap();
        let writer = qnc_registry::writer(&root).unwrap();
        assert_eq!(find(&root, "qnc://local/source/volume-1", &["qnc://local/source/volume-1/file/a.mxf".into()]).unwrap(), None);
        drop(writer);
        let _ = std::fs::remove_dir_all(&root);
    }
}

