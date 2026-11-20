//! A project folder must not go by an accidental delete. On Windows the folder gets an ACL
//! that denies delete and delete-child to the current user, passed down to everything in it;
//! the cache folders named by the caller (prerender, exports) get an explicit allow for their
//! contents, nearer than the deny, so their files may still go while the folders stay. The
//! folder is also hidden. `unlock` lifts all of it. Other systems: nothing is done yet, and
//! that is said. The programs `icacls` and `attrib` are the system's.

use std::path::Path;

/// Protects `folder`; `caches` are folders inside it whose files may be deleted.
pub fn lock(folder: &Path, caches: &[&str]) -> Result<(), String> {
    if !folder.is_dir() {
        return Err(qnc_text::fill("lock.failed", &[("path", &folder.display().to_string()), ("error", "not a folder")]));
    }
    sys::lock(folder, caches)
}

pub fn unlock(folder: &Path, caches: &[&str]) -> Result<(), String> {
    if !folder.is_dir() {
        return Ok(());
    }
    sys::unlock(folder, caches)
}

/// Whether `folder` is protected now.
pub fn is_locked(folder: &Path) -> Result<bool, String> {
    sys::is_locked(folder)
}

#[cfg(windows)]
mod sys {
    use std::path::Path;
    use std::process::Stdio;

    use qnc_launch::Priority;

    fn failed(folder: &Path, error: &str) -> String {
        qnc_text::fill("lock.failed", &[("path", &folder.display().to_string()), ("error", error)])
    }

    fn user() -> Result<String, String> {
        let name = std::env::var("USERNAME").map_err(|e| e.to_string())?;
        let domain = std::env::var("USERDOMAIN").unwrap_or_default();
        Ok(if domain.trim().is_empty() { name } else { format!("{domain}\\{name}") })
    }

    fn run(program: &str, folder: &Path, args: &[&str]) -> Result<(), String> {
        let status = qnc_launch::command(Path::new(program), Priority::Same).arg(folder).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|e| failed(folder, &e.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(failed(folder, &format!("{program} {}", args.join(" "))))
        }
    }

    fn has(folder: &Path, entry: &str) -> Result<bool, String> {
        let output = qnc_launch::command(Path::new("icacls"), Priority::Same).arg(folder).stderr(Stdio::null()).output().map_err(|e| failed(folder, &e.to_string()))?;
        let entry = entry.to_lowercase();
        Ok(String::from_utf8_lossy(&output.stdout).lines().any(|line| line.to_lowercase().contains(&entry)))
    }

    pub fn is_locked(folder: &Path) -> Result<bool, String> {
        has(folder, &format!("{}:(OI)(CI)(DENY)(DE,DC)", user()?))
    }

    pub fn lock(folder: &Path, caches: &[&str]) -> Result<(), String> {
        let user = user()?;
        if !is_locked(folder)? {
            run("icacls", folder, &["/deny", &format!("{user}:(OI)(CI)(DE,DC)")])?;
        }
        let allow = format!("{user}:(OI)(CI)(IO)(DE)");
        for cache in caches {
            let dir = folder.join(cache);
            if dir.is_dir() && !has(&dir, &allow)? {
                run("icacls", &dir, &["/grant", &allow])?;
            }
        }
        // Hidden is a second guard against a stray hand, not the protection.
        let _ = run("attrib", folder, &["+h"]);
        Ok(())
    }

    pub fn unlock(folder: &Path, caches: &[&str]) -> Result<(), String> {
        let user = user()?;
        for cache in caches {
            let dir = folder.join(cache);
            if dir.is_dir() {
                run("icacls", &dir, &["/remove:g", &user])?;
            }
        }
        run("icacls", folder, &["/remove:d", &user])?;
        let _ = run("attrib", folder, &["-h"]);
        Ok(())
    }
}

#[cfg(not(windows))]
mod sys {
    use std::path::Path;

    pub fn is_locked(_folder: &Path) -> Result<bool, String> {
        Ok(false)
    }

    pub fn lock(folder: &Path, _caches: &[&str]) -> Result<(), String> {
        Err(qnc_text::fill("lock.failed", &[("path", &folder.display().to_string()), ("error", "not supported on this system yet")]))
    }

    pub fn unlock(_folder: &Path, _caches: &[&str]) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_locked_folder_keeps_its_files_a_cache_lets_its_files_go_and_unlock_lifts_it() {
        let dir = std::env::temp_dir().join(format!("qnc-folder-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cache")).unwrap();
        std::fs::write(dir.join("kept.txt"), b"x").unwrap();
        std::fs::write(dir.join("cache").join("gone.txt"), b"x").unwrap();
        lock(&dir, &["cache"]).unwrap();
        assert!(is_locked(&dir).unwrap());
        assert!(std::fs::remove_file(dir.join("kept.txt")).is_err(), "a file of the project does not go");
        assert!(std::fs::remove_file(dir.join("cache").join("gone.txt")).is_ok(), "a file of a cache goes");
        assert!(std::fs::remove_dir_all(&dir).is_err(), "the folder does not go");
        unlock(&dir, &["cache"]).unwrap();
        assert!(!is_locked(&dir).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
