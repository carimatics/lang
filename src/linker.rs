use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::{fs, io};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkRequest {
    output: PathBuf,
    objects: Vec<PathBuf>,
    library_search_paths: Vec<PathBuf>,
    libraries: Vec<OsString>,
}

impl LinkRequest {
    pub fn new(output: impl Into<PathBuf>) -> Self {
        Self {
            output: output.into(),
            objects: Vec::new(),
            library_search_paths: Vec::new(),
            libraries: Vec::new(),
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

    pub fn with_library_search_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.push_library_search_path(path);
        self
    }

    pub fn push_library_search_path(&mut self, path: impl Into<PathBuf>) {
        self.library_search_paths.push(path.into());
    }

    pub fn library_search_paths(&self) -> &[PathBuf] {
        &self.library_search_paths
    }

    pub fn with_library(mut self, name: impl Into<OsString>) -> Self {
        self.push_library(name);
        self
    }

    pub fn push_library(&mut self, name: impl Into<OsString>) {
        self.libraries.push(name.into());
    }

    pub fn libraries(&self) -> &[OsString] {
        &self.libraries
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkerFlavor {
    Cc,
    MsvcLink,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemLinker {
    program: OsString,
    arguments: Vec<OsString>,
    flavor: LinkerFlavor,
}

impl SystemLinker {
    pub fn host() -> Self {
        if cfg!(target_env = "msvc") {
            Self::new_with_flavor("link.exe", LinkerFlavor::MsvcLink)
        } else {
            Self::new("cc")
        }
    }

    pub fn new(program: impl Into<OsString>) -> Self {
        Self::new_with_flavor(program, LinkerFlavor::Cc)
    }

    pub fn new_with_flavor(program: impl Into<OsString>, flavor: LinkerFlavor) -> Self {
        Self {
            program: program.into(),
            arguments: Vec::new(),
            flavor,
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

    pub const fn flavor(&self) -> LinkerFlavor {
        self.flavor
    }

    pub fn command(&self, request: &LinkRequest) -> Result<LinkCommand, LinkError> {
        self.validate(request)?;

        let mut arguments = self.arguments.clone();
        arguments.extend(self.request_arguments(request));

        Ok(LinkCommand {
            program: self.program.clone(),
            arguments,
        })
    }

    pub fn command_with_response_file(
        &self,
        request: &LinkRequest,
        response_file: impl AsRef<Path>,
    ) -> Result<LinkCommand, LinkError> {
        self.validate(request)?;
        let response_file = response_file.as_ref();
        self.validate_response_file_path(request, response_file)?;

        let mut response_argument = OsString::from("@");
        response_argument.push(response_file.as_os_str());
        let mut arguments = self.arguments.clone();
        arguments.push(response_argument);

        Ok(LinkCommand {
            program: self.program.clone(),
            arguments,
        })
    }

    pub fn link(&self, request: &LinkRequest) -> Result<(), LinkError> {
        let command = self.command(request)?;
        Self::execute(request, command)
    }

    /// Writes or replaces `response_file`, retains it after the linker exits,
    /// and invokes the linker with the generated `@file` argument.
    pub fn link_with_response_file(
        &self,
        request: &LinkRequest,
        response_file: impl AsRef<Path>,
    ) -> Result<(), LinkError> {
        let response_file = response_file.as_ref();
        let command = self.command_with_response_file(request, response_file)?;
        let contents = self.response_file_contents(request)?;
        fs::write(response_file, contents).map_err(|source| LinkError::WriteResponseFile {
            path: response_file.to_path_buf(),
            source,
        })?;
        Self::execute(request, command)
    }

    fn validate(&self, request: &LinkRequest) -> Result<(), LinkError> {
        if self.program.is_empty() {
            return Err(LinkError::EmptyProgram);
        }
        if request.output.as_os_str().is_empty() {
            return Err(LinkError::EmptyOutput);
        }
        if request.objects.is_empty() {
            return Err(LinkError::NoObjects);
        }
        if let Some(index) = request
            .objects
            .iter()
            .position(|object| object.as_os_str().is_empty())
        {
            return Err(LinkError::EmptyObjectPath { index });
        }
        if let Some(index) = request
            .library_search_paths
            .iter()
            .position(|path| path.as_os_str().is_empty())
        {
            return Err(LinkError::EmptyLibrarySearchPath { index });
        }
        if let Some(index) = request
            .libraries
            .iter()
            .position(|library| library.is_empty())
        {
            return Err(LinkError::EmptyLibraryName { index });
        }

        Ok(())
    }

    fn validate_response_file_path(
        &self,
        request: &LinkRequest,
        response_file: &Path,
    ) -> Result<(), LinkError> {
        if response_file.as_os_str().is_empty() {
            return Err(LinkError::EmptyResponseFilePath);
        }
        if response_file == request.output {
            return Err(LinkError::ResponseFileConflictsWithOutput);
        }
        if let Some(index) = request
            .objects
            .iter()
            .position(|object| object == response_file)
        {
            return Err(LinkError::ResponseFileConflictsWithObject { index });
        }

        Ok(())
    }

    fn request_arguments(&self, request: &LinkRequest) -> Vec<OsString> {
        let mut arguments = Vec::new();
        arguments.extend(
            request
                .objects
                .iter()
                .map(|object| object.as_os_str().to_os_string()),
        );
        for path in &request.library_search_paths {
            let mut argument = OsString::from(match self.flavor {
                LinkerFlavor::Cc => "-L",
                LinkerFlavor::MsvcLink => "/LIBPATH:",
            });
            argument.push(path.as_os_str());
            arguments.push(argument);
        }
        for library in &request.libraries {
            let mut argument = OsString::new();
            match self.flavor {
                LinkerFlavor::Cc => {
                    argument.push("-l");
                    argument.push(library);
                }
                LinkerFlavor::MsvcLink => {
                    argument.push(library);
                    argument.push(".lib");
                }
            }
            arguments.push(argument);
        }
        match self.flavor {
            LinkerFlavor::Cc => {
                arguments.push(OsString::from("-o"));
                arguments.push(request.output.as_os_str().to_os_string());
            }
            LinkerFlavor::MsvcLink => {
                let mut output_argument = OsString::from("/OUT:");
                output_argument.push(request.output.as_os_str());
                arguments.push(output_argument);
            }
        }

        arguments
    }

    fn response_file_contents(&self, request: &LinkRequest) -> Result<Vec<u8>, LinkError> {
        self.validate(request)?;

        let mut text = String::new();
        for (index, argument) in self.request_arguments(request).iter().enumerate() {
            let argument = argument
                .to_str()
                .ok_or(LinkError::NonUnicodeResponseFileArgument { index })?;
            if argument.contains('\0') {
                return Err(LinkError::NulInResponseFileArgument { index });
            }
            if argument.contains(['\r', '\n']) {
                return Err(LinkError::NewlineInResponseFileArgument { index });
            }
            match self.flavor {
                LinkerFlavor::Cc => push_gnu_response_argument(&mut text, argument),
                LinkerFlavor::MsvcLink => push_windows_response_argument(&mut text, argument),
            }
            text.push('\n');
        }

        match self.flavor {
            LinkerFlavor::Cc => Ok(text.into_bytes()),
            LinkerFlavor::MsvcLink => {
                let mut bytes = Vec::with_capacity((text.len() + 1) * 2);
                for code_unit in std::iter::once(0xfeff).chain(text.encode_utf16()) {
                    bytes.extend_from_slice(&code_unit.to_le_bytes());
                }
                Ok(bytes)
            }
        }
    }

    fn execute(request: &LinkRequest, command: LinkCommand) -> Result<(), LinkError> {
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

fn push_gnu_response_argument(output: &mut String, argument: &str) {
    output.push('"');
    for character in argument.chars() {
        if matches!(character, '\\' | '"') {
            output.push('\\');
        }
        output.push(character);
    }
    output.push('"');
}

fn push_windows_response_argument(output: &mut String, argument: &str) {
    output.push('"');
    let mut backslashes = 0;
    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }

        let count = if character == '"' {
            backslashes * 2 + 1
        } else {
            backslashes
        };
        output.extend(std::iter::repeat_n('\\', count));
        output.push(character);
        backslashes = 0;
    }
    output.extend(std::iter::repeat_n('\\', backslashes * 2));
    output.push('"');
}

#[derive(Debug)]
pub enum LinkError {
    EmptyProgram,
    EmptyOutput,
    NoObjects,
    EmptyObjectPath {
        index: usize,
    },
    EmptyLibrarySearchPath {
        index: usize,
    },
    EmptyLibraryName {
        index: usize,
    },
    EmptyResponseFilePath,
    ResponseFileConflictsWithOutput,
    ResponseFileConflictsWithObject {
        index: usize,
    },
    NonUnicodeResponseFileArgument {
        index: usize,
    },
    NulInResponseFileArgument {
        index: usize,
    },
    NewlineInResponseFileArgument {
        index: usize,
    },
    WriteResponseFile {
        path: PathBuf,
        source: io::Error,
    },
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
            Self::EmptyObjectPath { index } => {
                write!(
                    formatter,
                    "link request object path {index} must not be empty"
                )
            }
            Self::EmptyLibrarySearchPath { index } => write!(
                formatter,
                "link request library search path {index} must not be empty"
            ),
            Self::EmptyLibraryName { index } => {
                write!(
                    formatter,
                    "link request library name {index} must not be empty"
                )
            }
            Self::EmptyResponseFilePath => {
                formatter.write_str("link response-file path must not be empty")
            }
            Self::ResponseFileConflictsWithOutput => {
                formatter.write_str("link response-file path must differ from the output path")
            }
            Self::ResponseFileConflictsWithObject { index } => write!(
                formatter,
                "link response-file path must differ from object path {index}"
            ),
            Self::NonUnicodeResponseFileArgument { index } => write!(
                formatter,
                "link response-file argument {index} must be valid Unicode"
            ),
            Self::NulInResponseFileArgument { index } => write!(
                formatter,
                "link response-file argument {index} must not contain NUL"
            ),
            Self::NewlineInResponseFileArgument { index } => write!(
                formatter,
                "link response-file argument {index} must not contain a newline"
            ),
            Self::WriteResponseFile { path, source } => write!(
                formatter,
                "failed to write linker response file `{}`: {source}",
                path.display()
            ),
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
            Self::WriteResponseFile { source, .. } | Self::Spawn { source, .. } => Some(source),
            Self::EmptyProgram
            | Self::EmptyOutput
            | Self::NoObjects
            | Self::EmptyObjectPath { .. }
            | Self::EmptyLibrarySearchPath { .. }
            | Self::EmptyLibraryName { .. }
            | Self::EmptyResponseFilePath
            | Self::ResponseFileConflictsWithOutput
            | Self::ResponseFileConflictsWithObject { .. }
            | Self::NonUnicodeResponseFileArgument { .. }
            | Self::NulInResponseFileArgument { .. }
            | Self::NewlineInResponseFileArgument { .. }
            | Self::Failed { .. }
            | Self::MissingOutput { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::{Path, PathBuf};

    use super::{LinkError, LinkRequest, LinkerFlavor, SystemLinker};

    const MISSING_LINKER: &str = "lang-linker-test-command-that-does-not-exist";

    #[test]
    fn request_preserves_object_order_and_output() {
        let request = LinkRequest::new("build/program")
            .with_object("build/start.o")
            .with_object("build/main.o")
            .with_object("build/runtime.o")
            .with_library_search_path("build/lib")
            .with_library("support")
            .with_library("runtime");

        assert_eq!(request.output(), Path::new("build/program"));
        assert_eq!(
            request.objects(),
            [
                PathBuf::from("build/start.o"),
                PathBuf::from("build/main.o"),
                PathBuf::from("build/runtime.o"),
            ]
        );
        assert_eq!(request.library_search_paths(), [PathBuf::from("build/lib")]);
        assert_eq!(
            request.libraries(),
            [OsString::from("support"), OsString::from("runtime")]
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
            .with_object("build/main.o")
            .with_library_search_path("build/lib")
            .with_library("support")
            .with_library("runtime");

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
                OsString::from("-Lbuild/lib"),
                OsString::from("-lsupport"),
                OsString::from("-lruntime"),
                OsString::from("-o"),
                OsString::from("build/program"),
            ]
        );
    }

    #[test]
    fn msvc_command_uses_link_output_syntax() {
        let linker = SystemLinker::new_with_flavor("link.exe", LinkerFlavor::MsvcLink)
            .with_argument("/NOLOGO");
        let request = LinkRequest::new("build/program.exe")
            .with_object("build/start.obj")
            .with_object("build/main.obj")
            .with_library_search_path("build/lib")
            .with_library("support")
            .with_library("runtime");

        let command = linker.command(&request).unwrap();

        assert_eq!(linker.flavor(), LinkerFlavor::MsvcLink);
        assert_eq!(command.program(), OsStr::new("link.exe"));
        assert_eq!(
            command.arguments(),
            [
                OsString::from("/NOLOGO"),
                OsString::from("build/start.obj"),
                OsString::from("build/main.obj"),
                OsString::from("/LIBPATH:build/lib"),
                OsString::from("support.lib"),
                OsString::from("runtime.lib"),
                OsString::from("/OUT:build/program.exe"),
            ]
        );
    }

    #[test]
    fn cc_response_file_preserves_arguments_and_uses_gnu_escaping() {
        let linker = SystemLinker::new("xcrun")
            .with_argument("--sdk")
            .with_argument("macosx")
            .with_argument("clang");
        let request = LinkRequest::new("build/program")
            .with_object("build/start file.o")
            .with_object("build/quote\"and\\slash.o")
            .with_library_search_path("build/lib dir")
            .with_library("support");

        let command = linker
            .command_with_response_file(&request, "build/link arguments.rsp")
            .unwrap();
        let contents = String::from_utf8(linker.response_file_contents(&request).unwrap()).unwrap();

        assert_eq!(
            command.arguments(),
            [
                OsString::from("--sdk"),
                OsString::from("macosx"),
                OsString::from("clang"),
                OsString::from("@build/link arguments.rsp"),
            ]
        );
        assert_eq!(
            contents,
            concat!(
                "\"build/start file.o\"\n",
                "\"build/quote\\\"and\\\\slash.o\"\n",
                "\"-Lbuild/lib dir\"\n",
                "\"-lsupport\"\n",
                "\"-o\"\n",
                "\"build/program\"\n",
            )
        );
    }

    #[test]
    fn msvc_response_file_uses_windows_escaping_and_utf16() {
        let linker = SystemLinker::new_with_flavor("link.exe", LinkerFlavor::MsvcLink)
            .with_argument("/NOLOGO");
        let request = LinkRequest::new("build\\program.exe")
            .with_object("build\\start file.obj")
            .with_object("build\\trailing\\")
            .with_library_search_path("build\\lib dir")
            .with_library("support");

        let command = linker
            .command_with_response_file(&request, "build\\link arguments.rsp")
            .unwrap();
        let bytes = linker.response_file_contents(&request).unwrap();
        let (chunks, remainder) = bytes.as_chunks::<2>();
        assert!(remainder.is_empty());
        let code_units = chunks
            .iter()
            .map(|chunk| u16::from_le_bytes(*chunk))
            .collect::<Vec<_>>();

        assert_eq!(
            command.arguments(),
            [
                OsString::from("/NOLOGO"),
                OsString::from("@build\\link arguments.rsp"),
            ]
        );
        assert_eq!(code_units[0], 0xfeff);
        assert_eq!(
            String::from_utf16(&code_units[1..]).unwrap(),
            concat!(
                "\"build\\start file.obj\"\n",
                "\"build\\trailing\\\\\"\n",
                "\"/LIBPATH:build\\lib dir\"\n",
                "\"support.lib\"\n",
                "\"/OUT:build\\program.exe\"\n",
            )
        );
    }

    #[test]
    fn rejects_unsafe_response_file_paths_and_arguments() {
        let linker = SystemLinker::new("cc");
        let request = LinkRequest::new("program").with_object("main.o");

        assert!(matches!(
            linker.command_with_response_file(&request, ""),
            Err(LinkError::EmptyResponseFilePath)
        ));
        assert!(matches!(
            linker.command_with_response_file(&request, "program"),
            Err(LinkError::ResponseFileConflictsWithOutput)
        ));
        assert!(matches!(
            linker.command_with_response_file(&request, "main.o"),
            Err(LinkError::ResponseFileConflictsWithObject { index: 0 })
        ));

        let request = LinkRequest::new("program").with_object("main\nobject.o");
        assert!(matches!(
            linker.response_file_contents(&request),
            Err(LinkError::NewlineInResponseFileArgument { index: 0 })
        ));

        let request = LinkRequest::new("program").with_object("main\0object.o");
        assert!(matches!(
            linker.response_file_contents(&request),
            Err(LinkError::NulInResponseFileArgument { index: 0 })
        ));
    }

    #[test]
    fn reports_response_file_write_failures_before_spawning() {
        let request = LinkRequest::new("program").with_object("main.o");
        let error = SystemLinker::new(MISSING_LINKER)
            .link_with_response_file(&request, std::env::temp_dir())
            .expect_err("reports an unwritable response-file path");

        assert!(matches!(error, LinkError::WriteResponseFile { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_unicode_response_file_arguments() {
        use std::os::unix::ffi::OsStringExt;

        let object = OsString::from_vec(vec![b'm', b'a', b'i', b'n', 0xff, b'.', b'o']);
        let request = LinkRequest::new("program").with_object(PathBuf::from(object));
        let error = SystemLinker::new("cc")
            .response_file_contents(&request)
            .expect_err("rejects a non-Unicode response-file argument");

        assert!(matches!(
            error,
            LinkError::NonUnicodeResponseFileArgument { index: 0 }
        ));
    }

    #[test]
    fn host_linker_matches_the_toolchain_environment() {
        let linker = SystemLinker::host();

        #[cfg(target_env = "msvc")]
        {
            assert_eq!(linker.program(), OsStr::new("link.exe"));
            assert_eq!(linker.flavor(), LinkerFlavor::MsvcLink);
        }

        #[cfg(not(target_env = "msvc"))]
        {
            assert_eq!(linker.program(), OsStr::new("cc"));
            assert_eq!(linker.flavor(), LinkerFlavor::Cc);
        }
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
    fn rejects_an_empty_object_path_before_spawning() {
        let request = LinkRequest::new("program").with_object("");
        let error = SystemLinker::new(MISSING_LINKER)
            .link(&request)
            .expect_err("rejects an empty object path");

        assert!(matches!(error, LinkError::EmptyObjectPath { index: 0 }));
    }

    #[test]
    fn rejects_an_empty_library_search_path_before_spawning() {
        let request = LinkRequest::new("program")
            .with_object("main.o")
            .with_library_search_path("");
        let error = SystemLinker::new(MISSING_LINKER)
            .link(&request)
            .expect_err("rejects an empty library search path");

        assert!(matches!(
            error,
            LinkError::EmptyLibrarySearchPath { index: 0 }
        ));
    }

    #[test]
    fn rejects_an_empty_library_name_before_spawning() {
        let request = LinkRequest::new("program")
            .with_object("main.o")
            .with_library("");
        let error = SystemLinker::new(MISSING_LINKER)
            .link(&request)
            .expect_err("rejects an empty library name");

        assert!(matches!(error, LinkError::EmptyLibraryName { index: 0 }));
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
