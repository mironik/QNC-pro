//! The location browser of the computer (copied from the old QNC's `qnc-dir-browser`, its local part):
//! the disks of the computer and the folders of a path, listed without hidden and system ones;
//! a session binds each folder to a QNC URI (`qnc://local/source/<hash>`) that is its public
//! identity, and keeps the private path behind it. It draws nothing and knows no form.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

mod display;
mod listing;
pub use display::*;
pub use listing::*;

/// Private owner guard against binding a replacement volume to an old media identity.
/// OSes without an exposed volume serial require an explicitly registered source.
pub fn verify_local_volume_serial(path: &Path, expected: &str) -> Result<(), String> {
    if expected.trim().is_empty() {
        return Ok(());
    }
    let path = path
        .canonicalize()
        .map_err(|_| "source volume unavailable")?;
    let volume = list_roots().into_iter().find(|r| {
        r.local_path
            .canonicalize()
            .is_ok_and(|root| path.starts_with(root))
    });
    if let Some(actual) = volume.and_then(|v| v.serial_number) {
        if !actual.eq_ignore_ascii_case(expected) {
            return Err("source volume serial changed; owner binding must be corrected".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryListRequest {
    pub directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub name: String,
    pub local_path: PathBuf,
    pub is_dir: bool,
    pub serial_number: Option<String>,
    pub volume_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryListing {
    pub directory: PathBuf,
    pub parent: Option<PathBuf>,
    pub roots: bool,
    pub entries: Vec<DirectoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BrowserEntry {
    pub name: String,
    pub qnc_uri: String,
    pub serial_number: String,
    pub volume_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BrowserCrumb {
    pub label: String,
    pub qnc_uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BrowserState {
    pub roots: bool,
    pub path_label: String,
    pub current_uri: Option<String>,
    pub parent_available: bool,
    pub breadcrumbs: Vec<BrowserCrumb>,
    pub entries: Vec<BrowserEntry>,
}

#[derive(Debug, Default)]
pub struct DirectoryBrowserSession {
    current_directory: Option<PathBuf>,
    current_uri: Option<String>,
    parent_directory: Option<PathBuf>,
    bindings: HashMap<String, PathBuf>,
}

impl DirectoryBrowserSession {
    pub fn load_roots(&mut self) -> Result<BrowserState, String> {
        self.current_directory = None;
        self.current_uri = None;
        self.parent_directory = None;
        let listing = list_directory(&DirectoryListRequest {
            directory: PathBuf::new(),
        })?;
        self.map_listing(listing)
    }

    pub fn open_uri(&mut self, uri: &str) -> Result<BrowserState, String> {
        let path = self
            .bindings
            .get(uri)
            .cloned()
            .ok_or_else(|| format!("unknown QNC location URI: {uri}"))?;
        let listing = list_directory(&DirectoryListRequest { directory: path })?;
        self.map_listing(listing)
    }

    pub fn open_private_path(&mut self, path: impl AsRef<Path>) -> Result<BrowserState, String> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return self.load_roots();
        }
        let listing = list_directory(&DirectoryListRequest {
            directory: path.to_path_buf(),
        })?;
        self.map_listing(listing)
    }

    pub fn open_parent(&mut self) -> Result<BrowserState, String> {
        let Some(parent) = self.parent_directory.clone() else {
            return self.load_roots();
        };
        let listing = list_directory(&DirectoryListRequest { directory: parent })?;
        self.map_listing(listing)
    }

    pub fn path_for_uri(&self, uri: &str) -> Option<PathBuf> {
        self.bindings.get(uri).cloned()
    }

    fn map_listing(&mut self, listing: DirectoryListing) -> Result<BrowserState, String> {
        if listing.roots {
            self.current_directory = None;
            self.current_uri = None;
            self.parent_directory = None;
        } else {
            self.current_directory = Some(listing.directory.clone());
            self.current_uri = Some(self.bind_path(&listing.directory));
            self.parent_directory = listing.parent.clone();
        }

        let entries = listing
            .entries
            .iter()
            .map(|entry| BrowserEntry {
                name: display_entry_name(&entry.name),
                qnc_uri: self.bind_path(&entry.local_path),
                serial_number: entry.serial_number.clone().unwrap_or_default(),
                volume_name: entry.volume_name.clone().unwrap_or_default(),
            })
            .collect::<Vec<_>>();

        let path_label = if listing.roots {
            String::new()
        } else {
            display_path_label(&listing.directory)
        };
        let breadcrumbs = if listing.roots {
            Vec::new()
        } else {
            self.breadcrumbs_for_path(&listing.directory)
        };

        Ok(BrowserState {
            roots: listing.roots,
            path_label,
            current_uri: self.current_uri.clone(),
            parent_available: self.parent_directory.is_some(),
            breadcrumbs,
            entries,
        })
    }

    fn breadcrumbs_for_path(&mut self, path: &Path) -> Vec<BrowserCrumb> {
        browser_crumb_paths(path)
            .into_iter()
            .map(|(label, path)| BrowserCrumb {
                label,
                qnc_uri: self.bind_path(&path),
            })
            .collect()
    }

    fn bind_path(&mut self, path: &Path) -> String {
        let normalized = normalize_path_for_identity(path);
        let uri = format!("qnc://local/source/{:016x}", fnv1a64(&normalized));
        self.bindings
            .entry(uri.clone())
            .or_insert_with(|| path.to_path_buf());
        uri
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserAction {
    Kind(usize),
    Open(String),
    Parent,
    Roots,
}

/// What the user asked of the browser with its confirm bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserAnswer {
    Browser(BrowserAction),
    Confirm,
    Cancel,
}
#[cfg(test)]
mod tests;
