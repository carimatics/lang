use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkRequest {
    output: PathBuf,
    objects: Vec<PathBuf>,
}

impl LinkRequest {
    pub fn new(output: impl Into<PathBuf>) -> Self {
        Self {
            output: output.into(),
            objects: Vec::new(),
        }
    }

    pub fn with_object(mut self, object: impl Into<PathBuf>) -> Self {
        self.push_object(object);
        self
    }

    pub fn push_object(&mut self, object: impl Into<PathBuf>) {
        self.objects.push(object.into());
    }

    pub fn output(&self) -> &Path {
        &self.output
    }

    pub fn objects(&self) -> &[PathBuf] {
        &self.objects
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemLinker {
    program: OsString,
}

impl SystemLinker {
    pub fn host() -> Self {
        Self::new("cc")
    }

    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn program(&self) -> &OsStr {
        &self.program
    }

    pub fn link(&self, request: &LinkRequest) -> Result<(), LinkError> {
        if request.output.as_os_str().is_empty() {
            return Err(LinkError::EmptyOutput);
        }
        if request.objects.is_empty() {
            return Err(LinkError::NoObjects);
        }

        let output = Command::new(&self.program)
            .args(&request.objects)
            .arg("-o")
            .arg(&request.output)
            .output()
            .map_err(|source| LinkError::Spawn {
                program: self.program.clone(),
                source,
            })?;

        if !output.status.success() {
            return Err(LinkError::Failed {
                program: self.program.clone(),
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            });
        }
        if !request.output.is_file() {
            return Err(LinkError::MissingOutput(request.output.clone()));
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum LinkError {
    EmptyOutput,
    NoObjects,
    Spawn {
        program: OsString,
        source: io::Error,
    },
    Failed {
        program: OsString,
        status: ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    MissingOutput(PathBuf),
}

impl Display for LinkError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyOutput => formatter.write_str("link output path must not be empty"),
            Self::NoObjects => formatter.write_str("link request must contain at least one object"),
            Self::Spawn { program, source } => {
                write!(
                    formatter,
                    "failed to start system linker `{}`: {source}",
                    program.to_string_lossy()
                )
            }
            Self::Failed {
                program,
                status,
                stdout,
                stderr,
            } => write!(
                formatter,
                "system linker `{}` failed with {status}:\nstdout:\n{}\nstderr:\n{}",
                program.to_string_lossy(),
                String::from_utf8_lossy(stdout),
                String::from_utf8_lossy(stderr)
            ),
            Self::MissingOutput(path) => write!(
                formatter,
                "system linker reported success without creating `{}`",
                path.display()
            ),
        }
    }
}

impl Error for LinkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Spawn { source, .. } => Some(source),
            Self::EmptyOutput | Self::NoObjects | Self::Failed { .. } | Self::MissingOutput(_) => {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{LinkError, LinkRequest, SystemLinker};

    const MISSING_LINKER: &str = "lang-linker-test-command-that-does-not-exist";

    #[test]
    fn request_preserves_object_order_and_output() {
        let request = LinkRequest::new("build/program")
            .with_object("build/start.o")
            .with_object("build/main.o")
            .with_object("build/runtime.o");

        assert_eq!(request.output(), Path::new("build/program"));
        assert_eq!(
            request.objects(),
            [
                PathBuf::from("build/start.o"),
                PathBuf::from("build/main.o"),
                PathBuf::from("build/runtime.o"),
            ]
        );
    }

    #[test]
    fn rejects_an_empty_output_before_spawning() {
        let request = LinkRequest::new("").with_object("main.o");
        let error = SystemLinker::new(MISSING_LINKER)
            .link(&request)
            .expect_err("rejects an empty output path");

        assert!(matches!(error, LinkError::EmptyOutput));
    }

    #[test]
    fn rejects_a_request_without_objects_before_spawning() {
        let request = LinkRequest::new("program");
        let error = SystemLinker::new(MISSING_LINKER)
            .link(&request)
            .expect_err("rejects a request without objects");

        assert!(matches!(error, LinkError::NoObjects));
    }

    #[test]
    fn reports_linker_spawn_failures() {
        let request = LinkRequest::new("program").with_object("main.o");
        let error = SystemLinker::new(MISSING_LINKER)
            .link(&request)
            .expect_err("reports an unavailable linker");

        assert!(matches!(error, LinkError::Spawn { .. }));
    }
}
