//! Atomic no-replace directory publication on the two tier-1 build hosts.
use std::{io, path::Path};
#[cfg(target_os = "macos")]
pub(crate) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn renamex_np(
            from: *const std::ffi::c_char,
            to: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    let from =
        CString::new(from.as_os_str().as_bytes()).map_err(|_| io::ErrorKind::InvalidInput)?;
    let to = CString::new(to.as_os_str().as_bytes()).map_err(|_| io::ErrorKind::InvalidInput)?;
    // SAFETY: both complete NUL-terminated paths remain live; RENAME_EXCL
    // requests atomic refusal if any destination already exists.
    if unsafe { renamex_np(from.as_ptr(), to.as_ptr(), 4) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(target_os = "windows")]
pub(crate) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let path = |p: &Path| -> io::Result<Vec<u16>> {
        let mut s: Vec<_> = p.as_os_str().encode_wide().collect();
        if s.contains(&0) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        s.push(0);
        Ok(s)
    };
    let from = path(from)?;
    let to = path(to)?;
    // SAFETY: live terminated paths; REPLACE_EXISTING is deliberately absent.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
        )
    } != 0
    {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn rename(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}
