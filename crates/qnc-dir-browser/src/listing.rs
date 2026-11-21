//! Listing the filesystem: directories, roots, volume identity (the private side of the browser).

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{DirectoryEntry, DirectoryListRequest, DirectoryListing};

pub fn list_directory(request: &DirectoryListRequest) -> Result<DirectoryListing, String> {
    if request.directory.as_os_str().is_empty() {
        return Ok(DirectoryListing {
            directory: PathBuf::new(),
            parent: None,
            roots: true,
            entries: list_roots(),
        });
    }

    let path = normalize_list_path(&request.directory)?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("path not accessible: {error}"))?;
    if !canonical.is_dir() {
        return Err("not a directory".to_string());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&canonical).map_err(|error| format!("read_dir failed: {error}"))? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.is_empty() {
            continue;
        }
        let path = entry.path();
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_dir() || is_hidden_or_system(&name, &metadata) {
            continue;
        }
        entries.push(DirectoryEntry {
            name,
            local_path: path,
            is_dir: true,
            serial_number: None,
            volume_name: None,
        });
    }
    entries.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(DirectoryListing {
        parent: parent_path(&canonical),
        directory: canonical,
        roots: false,
        entries,
    })
}

pub fn list_roots() -> Vec<DirectoryEntry> {
    #[cfg(windows)]
    {
        let mut roots = Vec::new();
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            let path = PathBuf::from(&root);
            if path.is_dir() {
                let identity = volume_identity(&path);
                roots.push(DirectoryEntry {
                    name: root,
                    local_path: path,
                    is_dir: true,
                    serial_number: identity.serial_number,
                    volume_name: identity.volume_name,
                });
            }
        }
        roots
    }
    #[cfg(not(windows))]
    {
        vec![DirectoryEntry {
            name: "/".to_string(),
            local_path: PathBuf::from("/"),
            is_dir: true,
            serial_number: None,
            volume_name: None,
        }]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct VolumeIdentity {
    serial_number: Option<String>,
    volume_name: Option<String>,
}

#[cfg(windows)]
fn volume_identity(root: &Path) -> VolumeIdentity {
    use std::{os::windows::ffi::OsStrExt, ptr::null_mut};
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;

    let root_wide = root
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut name = vec![0u16; 260];
    let mut serial = 0u32;
    let ok = unsafe {
        GetVolumeInformationW(
            root_wide.as_ptr(),
            name.as_mut_ptr(),
            name.len() as u32,
            &mut serial,
            null_mut(),
            null_mut(),
            null_mut(),
            0,
        )
    };

    if ok == 0 {
        return VolumeIdentity::default();
    }

    let end = name
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(name.len());
    let volume_name = String::from_utf16_lossy(&name[..end]);
    VolumeIdentity {
        serial_number: Some(format!("{serial:08x}")),
        volume_name: if volume_name.trim().is_empty() {
            None
        } else {
            Some(volume_name)
        },
    }
}

#[cfg(not(windows))]
fn volume_identity(_root: &Path) -> VolumeIdentity {
    VolumeIdentity::default()
}

pub(crate) fn normalize_list_path(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("empty path".to_string());
    }
    if path.to_string_lossy().contains('\0') {
        return Err("invalid path".to_string());
    }
    if !path.is_absolute() {
        return Err("path must be absolute".to_string());
    }
    Ok(path.to_path_buf())
}

pub(crate) fn parent_path(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    #[cfg(windows)]
    {
        let display = parent.to_string_lossy();
        if display.len() == 2 && display.as_bytes()[1] == b':' {
            return Some(PathBuf::from(format!("{display}\\")));
        }
    }
    Some(parent.to_path_buf())
}

fn is_hidden_or_system(name: &str, metadata: &fs::Metadata) -> bool {
    if name.starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
        let attrs = metadata.file_attributes();
        attrs & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

