//! The post signal between the player and the monitor, as the program raises it: on Windows a
//! named event derived from the mailbox path, on Unix a fifo beside it. Not a clock.

use std::path::Path;
use std::time::Duration;

pub(crate) struct Wake {
    inner: sys::Inner,
}

impl Wake {
    pub(crate) fn create(path: &Path) -> Result<Wake, String> {
        Ok(Wake { inner: sys::Inner::create(path)? })
    }

    pub(crate) fn open(path: &Path) -> Result<Wake, String> {
        Ok(Wake { inner: sys::Inner::open(path)? })
    }

    pub(crate) fn signal(&self) -> Result<(), String> {
        self.inner.signal()
    }

    pub(crate) fn wait(&self, timeout: Duration) -> Result<bool, String> {
        self.inner.wait(timeout)
    }
}

#[cfg(windows)]
mod sys {
    use std::collections::hash_map::DefaultHasher;
    use std::ffi::OsString;
    use std::hash::{Hash, Hasher};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{CreateEventW, ResetEvent, SetEvent, WaitForSingleObject};

    pub(crate) struct Inner {
        handle: HANDLE,
    }

    // SAFETY: an event handle may be used from any thread.
    unsafe impl Send for Inner {}
    unsafe impl Sync for Inner {}

    impl Inner {
        pub(crate) fn create(path: &Path) -> Result<Inner, String> {
            open_named(path)
        }

        pub(crate) fn open(path: &Path) -> Result<Inner, String> {
            open_named(path)
        }

        pub(crate) fn signal(&self) -> Result<(), String> {
            // SAFETY: a live event handle.
            if unsafe { SetEvent(self.handle) } == 0 {
                return Err("frame map wake signal failed".into());
            }
            Ok(())
        }

        pub(crate) fn wait(&self, timeout: Duration) -> Result<bool, String> {
            let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
            // SAFETY: a live event handle.
            match unsafe { WaitForSingleObject(self.handle, ms) } {
                WAIT_OBJECT_0 => {
                    // SAFETY: a live event handle.
                    let _ = unsafe { ResetEvent(self.handle) };
                    Ok(true)
                }
                WAIT_TIMEOUT => Ok(false),
                _ => Err("frame map wake wait failed".into()),
            }
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            if !self.handle.is_null() {
                // SAFETY: closed once, here.
                unsafe { CloseHandle(self.handle) };
            }
        }
    }

    /// The player derives the same name from the same path (hash of the path, `Local\qnc-frm-`).
    fn open_named(path: &Path) -> Result<Inner, String> {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        let name = format!("Local\\qnc-frm-{:016x}", hasher.finish());
        let wide: Vec<u16> = OsString::from(name).encode_wide().chain(std::iter::once(0)).collect();
        // SAFETY: a valid, nul-terminated wide name; a manual-reset event, not signalled.
        let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, wide.as_ptr()) };
        if handle.is_null() || handle == (-1isize as HANDLE) {
            return Err("frame map wake event unavailable".into());
        }
        Ok(Inner { handle })
    }
}

#[cfg(unix)]
mod sys {
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::path::Path;
    use std::time::Duration;

    pub(crate) struct Inner {
        file: File,
    }

    impl Inner {
        pub(crate) fn create(path: &Path) -> Result<Inner, String> {
            let fifo = path.with_extension("wake");
            let name = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
            // SAFETY: a valid C string.
            match unsafe { libc::mkfifo(name.as_ptr(), 0o600) } {
                0 => {}
                _ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::AlreadyExists => {}
                _ => return Err(std::io::Error::last_os_error().to_string()),
            }
            let file = OpenOptions::new().read(true).write(true).open(&fifo).map_err(|e| e.to_string())?;
            Ok(Inner { file })
        }

        pub(crate) fn open(path: &Path) -> Result<Inner, String> {
            Inner::create(path)
        }

        pub(crate) fn signal(&self) -> Result<(), String> {
            let mut file = self.file.try_clone().map_err(|e| e.to_string())?;
            file.write_all(&[1]).map_err(|e| e.to_string())
        }

        pub(crate) fn wait(&self, timeout: Duration) -> Result<bool, String> {
            let mut pollfd = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            let ms = libc::c_int::try_from(timeout.as_millis()).unwrap_or(libc::c_int::MAX);
            // SAFETY: one valid pollfd.
            match unsafe { libc::poll(&mut pollfd, 1, ms) } {
                0 => Ok(false),
                n if n > 0 => {
                    let mut file = self.file.try_clone().map_err(|e| e.to_string())?;
                    let mut discard = [0u8; 32];
                    while let Ok(n) = file.read(&mut discard) {
                        if n < discard.len() {
                            break;
                        }
                    }
                    Ok(true)
                }
                _ => Err(std::io::Error::last_os_error().to_string()),
            }
        }
    }
}
