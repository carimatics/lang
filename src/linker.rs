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
pub struct LinkCommand {
    program: OsString,
    arguments: Vec<OsString>,
}

impl LinkCommand {
    pub fn program(&self) -> &OsStr {
        &self.program
    }

    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }
}

impl Display for LinkCommand {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.program)?;
        for argument in &self.arguments {
            write!(formatter, " {argument:?}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemLinker {
    program: OsString,
    arguments: Vec<OsString>,
}

impl SystemLinker {
    pub fn host() -> Self {
        Self::new("cc")
    }

    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            arguments: Vec::new(),
        }
    }

    pub fn with_argument(mut self, argument: impl Into<OsString>) -> Self {
        self.push_argument(argument);
        self
    }

    pub fn push_argument(&mut self, argument: impl Into<OsString>) {
        self.arguments.push(argument.into());
    }

    pub fn program(&self) -> &OsStr {
        &self.program
    }

    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    pub fn command(&self, request: &LinkRequest) -> Result<LinkCommand, LinkError> {
        if self.program.is_empty() {
            return Err(LinkError::EmptyProgram);
        }
        if request.output.as_os_str().is_empty() {
            return Err(LinkError::EmptyOutput);
        }
        if request.objects.is_empty() {
            return Err(LinkError::NoObjects);
        }

        let mut arguments = self.arguments.clone();
        arguments.extend(
            request
                .objects
                .iter()
                .map(|object| object.as_os_str().to_os_string()),
        );
        arguments.push(OsString::from("-o"));
        arguments.push(request.output.as_os_str().to_os_string());

        Ok(LinkCommand {
            program: self.program.clone(),
            arguments,
        })
    }

    pub fn link(&self, request: &LinkRequest) -> Result<(), LinkError> {
        let command = self.command(request)?;
        let output = Command::new(command.program())
            .args(command.arguments())
            .output()
            .map_err(|source| LinkError::Spawn {
                command: command.clone(),
                source,
            })?;

        if !output.status.success() {
            return Err(LinkError::Failed {
                command,
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            });
        }
        if !request.output.is_file() {
            return Err(LinkError::MissingOutput {
                command,
                path: request.output.clone(),
            });
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum LinkError {
    EmptyProgram,
    EmptyOutput,
    NoObjects,
    Spawn {
        command: LinkCommand,
        source: io::Error,
    },
    Failed {
        command: LinkCommand,
        status: ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    MissingOutput {
        command: LinkCommand,
        path: PathBuf,
    },
}

impl Display for LinkError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyProgram => formatter.write_str("system linker program must not be empty"),
            Self::EmptyOutput => formatter.write_str("link output path must not be empty"),
            Self::NoObjects => formatter.write_str("link request must contain at least one object"),
            Self::Spawn { command, source } => {
                write!(
                    formatter,
                    "failed to start system linker command {command}: {source}"
                )
            }
            Self::Failed {
                command,
                status,
                stdout,
                stderr,
            } => write!(
                formatter,
                "system linker command {command} failed with {status}:\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(stdout),
                String::from_utf8_lossy(stderr)
            ),
            Self::MissingOutput { command, path } => write!(
                formatter,
                "system linker command {command} reported success without creating `{}`",
                path.display()
            ),
        }
    }
}

impl Error for LinkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Spawn { source, .. } => Some(source),
            Self::EmptyProgram
            | Self::EmptyOutput
            | Self::NoObjects
            | Self::Failed { .. }
            | Self::MissingOutput { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
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
    fn command_preserves_driver_and_argument_order() {
        let linker = SystemLinker::new("xcrun")
            .with_argument("--sdk")
            .with_argument("macosx")
            .with_argument("clang");
        let request = LinkRequest::new("build/program")
            .with_object("build/start.o")
            .with_object("build/main.o");

        let command = linker.command(&request).unwrap();

        assert_eq!(linker.program(), OsStr::new("xcrun"));
        assert_eq!(
            linker.arguments(),
            [
                OsString::from("--sdk"),
                OsString::from("macosx"),
                OsString::from("clang"),
            ]
        );
        assert_eq!(command.program(), OsStr::new("xcrun"));
        assert_eq!(
            command.arguments(),
            [
                OsString::from("--sdk"),
                OsString::from("macosx"),
                OsString::from("clang"),
                OsString::from("build/start.o"),
                OsString::from("build/main.o"),
                OsString::from("-o"),
                OsString::from("build/program"),
            ]
        );
    }

    #[test]
    fn rejects_an_empty_linker_program_before_spawning() {
        let request = LinkRequest::new("program").with_object("main.o");
        let error = SystemLinker::new("")
            .link(&request)
            .expect_err("rejects an empty linker program");

        assert!(matches!(error, LinkError::EmptyProgram));
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

        let LinkError::Spawn { command, .. } = error else {
            panic!("expected a spawn failure");
        };
        assert_eq!(command.program(), OsStr::new(MISSING_LINKER));
        assert_eq!(
            command.arguments(),
            [
                OsString::from("main.o"),
                OsString::from("-o"),
                OsString::from("program"),
            ]
        );
    }
}
