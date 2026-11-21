use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn empty_path_returns_roots() {
        let listing = list_directory(&DirectoryListRequest {
            directory: PathBuf::new(),
        })
        .expect("roots");

        assert!(listing.roots);
        assert!(listing.parent.is_none());
        assert!(!listing.entries.is_empty());
    }

    #[test]
    fn browser_session_exposes_qnc_uri_state() {
        let mut session = DirectoryBrowserSession::default();
        let state = session.load_roots().expect("roots");

        assert!(state.roots);
        assert!(state
            .entries
            .iter()
            .all(|entry| entry.qnc_uri.starts_with("qnc://local/source/")));
        assert!(state
            .entries
            .iter()
            .all(|entry| !entry.name.contains("\\\\?\\")));
    }

    #[test]
    fn private_start_path_returns_browser_state_with_current_uri() {
        let root = temp_root("session");
        fs::create_dir_all(root.join("Child")).expect("child dir");

        let mut session = DirectoryBrowserSession::default();
        let state = session.open_private_path(&root).expect("state");

        assert!(!state.roots);
        assert!(state.current_uri.is_some());
        assert!(
            state
                .entries
                .iter()
                .any(|entry| entry.name == "Child"
                    && entry.qnc_uri.starts_with("qnc://local/source/"))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn browser_breadcrumbs_use_qnc_uri_not_raw_path() {
        let root = temp_root("crumbs");
        let nested = root.join("A").join("B");
        fs::create_dir_all(&nested).expect("nested dir");

        let mut session = DirectoryBrowserSession::default();
        let state = session.open_private_path(&nested).expect("state");

        assert!(!state.breadcrumbs.is_empty());
        assert!(state
            .breadcrumbs
            .iter()
            .all(|crumb| crumb.qnc_uri.starts_with("qnc://local/source/")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn display_labels_hide_windows_extended_prefix() {
        assert_eq!(display_entry_name("\\\\?\\G:\\"), "G:");
        assert_eq!(display_path_label(Path::new("\\\\?\\G:\\")), "G:");
        assert_eq!(display_private_path(Path::new("\\\\?\\G:\\")), "G:\\");
        assert_eq!(display_entry_name("/"), "/");
    }

    #[test]
    fn lists_only_visible_directories_without_media_scan() {
        let root = temp_root("list");
        fs::create_dir_all(root.join("B_dir")).expect("b dir");
        fs::create_dir_all(root.join("a_dir")).expect("a dir");
        fs::create_dir_all(root.join(".hidden")).expect("hidden dir");
        fs::write(root.join("clip.mp4"), "").expect("file");
        let listing = list_directory(&DirectoryListRequest {
            directory: root.clone(),
        })
        .expect("listing");

        assert_eq!(
            listing
                .entries
                .iter()
                .map(|entry| (entry.name.as_str(), entry.is_dir))
                .collect::<Vec<_>>(),
            [("a_dir", true), ("B_dir", true)]
        );
        assert!(!listing.roots);
        assert_eq!(
            listing.directory,
            root.canonicalize().expect("canonical root")
        );
        let _ = fs::remove_dir_all(root);
    }

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "qnc_dir_browser_{label}_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ))
    }
