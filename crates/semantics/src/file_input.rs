//! Host-side admission for bounded source and artifact readers.
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Open a regular file without waiting for a FIFO writer on Unix.
/// Validate the opened handle, rather than a racy path metadata snapshot.
/// This does not establish path authority or impose a byte limit; callers do both.
pub fn open_regular(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "input must be a regular file",
        ));
    }
    Ok(file)
}
