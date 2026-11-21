//! One copy of a program per session (copied from the old QNC's `qnc-tray::OneCopy`): a named
//! mutex of this session is taken when the program starts; a second copy finds it taken and
//! ends. The handle lives as long as the program.

pub struct OneCopy {
    #[cfg(windows)]
    _handle: windows::Handle,
}

impl OneCopy {
    /// Takes `name` for this program: `None` when another copy already has it.
    pub fn take(name: &str) -> Result<Option<Self>, String> {
        #[cfg(windows)]
        {
            Ok(windows::take(name)?.map(|handle| Self { _handle: handle }))
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            Err(qnc_text::text("one_copy.not_on_this_system").to_string())
        }
    }
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    pub struct Handle(HANDLE);

    // SAFETY: the handle is only closed, once, by its owner.
    unsafe impl Send for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle this copy made.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn take(name: &str) -> Result<Option<Handle>, String> {
        let name: Vec<u16> = format!("Local\\{name}").encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: a named mutex of this session; the name lives through the call.
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
            if handle.is_null() {
                return Err(qnc_text::fill("one_copy.failed", &[("error", &GetLastError().to_string())]));
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                return Ok(None);
            }
            Ok(Some(Handle(handle)))
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn the_second_copy_finds_the_name_taken_until_the_first_lets_go() {
        let name = format!("qnc-one-copy-test-{}", std::process::id());
        let first = super::OneCopy::take(&name).unwrap();
        assert!(first.is_some());
        assert!(super::OneCopy::take(&name).unwrap().is_none());
        drop(first);
        assert!(super::OneCopy::take(&name).unwrap().is_some());
    }
}
