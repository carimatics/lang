use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt::{Display, Formatter};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::{env, fs, io};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkTarget {
    MacosAarch64,
    WindowsX86_64Msvc,
}

impl LinkTarget {
    pub const fn host() -> Option<Self> {
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Self::MacosAarch64)
        } else if cfg!(all(
            target_os = "windows",
            target_arch = "x86_64",
            target_env = "msvc"
        )) {
            Some(Self::WindowsX86_64Msvc)
        } else {
            None
        }
    }

    pub fn from_triple(triple: &str) -> Result<Self, UnsupportedLinkTarget> {
        match triple {
            "aarch64-apple-darwin" => Ok(Self::MacosAarch64),
            "x86_64-pc-windows-msvc" => Ok(Self::WindowsX86_64Msvc),
            _ => Err(UnsupportedLinkTarget {
                triple: triple.to_owned(),
            }),
        }
    }

    pub const fn triple(self) -> &'static str {
        match self {
            Self::MacosAarch64 => "aarch64-apple-darwin",
            Self::WindowsX86_64Msvc => "x86_64-pc-windows-msvc",
        }
    }

    pub const fn linker_flavor(self) -> LinkerFlavor {
        match self {
            Self::MacosAarch64 => LinkerFlavor::Cc,
            Self::WindowsX86_64Msvc => LinkerFlavor::MsvcLink,
        }
    }

    pub const fn linker_candidates(self) -> &'static [&'static str] {
        match self {
            Self::MacosAarch64 => &["cc", "clang", "gcc"],
            Self::WindowsX86_64Msvc => &["link.exe"],
        }
    }

    pub const fn default_linker_program(self) -> &'static str {
        self.linker_candidates()[0]
    }
}

impl std::str::FromStr for LinkTarget {
    type Err = UnsupportedLinkTarget;

    fn from_str(triple: &str) -> Result<Self, Self::Err> {
        Self::from_triple(triple)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsupportedLinkTarget {
    triple: String,
}

impl UnsupportedLinkTarget {
    pub fn triple(&self) -> &str {
        &self.triple
    }
}

impl Display for UnsupportedLinkTarget {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "unsupported linker target triple {:?}; supported triples are {:?} and {:?}",
            self.triple,
            LinkTarget::MacosAarch64.triple(),
            LinkTarget::WindowsX86_64Msvc.triple()
        )
    }
}

impl Error for UnsupportedLinkTarget {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkerDiscoveryError {
    MissingPath,
    NotFound {
        candidates: Vec<OsString>,
        search_path: OsString,
        flavor: LinkerFlavor,
    },
}

impl Display for LinkerDiscoveryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPath => formatter.write_str(
                "PATH is not set for linker discovery; configure the target C toolchain on PATH",
            ),
            Self::NotFound {
                candidates,
                search_path,
                flavor,
            } => {
                formatter.write_str("no linker candidate found on PATH ")?;
                write!(formatter, "{search_path:?}; searched")?;
                for candidate in candidates {
                    write!(formatter, " {candidate:?}")?;
                }
                let help = match flavor {
                    LinkerFlavor::Cc => {
                        "install a C toolchain and add cc, clang, or gcc to PATH (macOS: Xcode Command Line Tools)"
                    }
                    LinkerFlavor::MsvcLink => {
                        "install the Visual Studio C++ Build Tools and use an x64 Developer Command Prompt so link.exe and Windows SDK libraries are available"
                    }
                };
                write!(formatter, "; {help}")
            }
        }
    }
}

impl Error for LinkerDiscoveryError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemLinker {
    program: OsString,
    arguments: Vec<OsString>,
    flavor: LinkerFlavor,
}

impl SystemLinker {
    pub fn host() -> Self {
        if let Some(target) = LinkTarget::host() {
            Self::for_target(target)
        } else if cfg!(target_env = "msvc") {
            Self::new_with_flavor("link.exe", LinkerFlavor::MsvcLink)
        } else {
            Self::new("cc")
        }
    }

    pub fn for_target(target: LinkTarget) -> Self {
        Self::new_with_flavor(target.default_linker_program(), target.linker_flavor())
    }

    pub fn discover_host() -> Result<Self, LinkerDiscoveryError> {
        let search_path = env::var_os("PATH").ok_or(LinkerDiscoveryError::MissingPath)?;
        Self::discover_host_in(search_path)
    }

    pub fn discover_host_in(search_path: impl AsRef<OsStr>) -> Result<Self, LinkerDiscoveryError> {
        let (candidates, flavor) = host_linker_candidates();
        Self::discover_in(search_path.as_ref(), candidates, flavor)
    }

    pub fn discover_target(target: LinkTarget) -> Result<Self, LinkerDiscoveryError> {
        let search_path = env::var_os("PATH").ok_or(LinkerDiscoveryError::MissingPath)?;
        Self::discover_target_in(target, search_path)
    }

    pub fn discover_target_in(
        target: LinkTarget,
        search_path: impl AsRef<OsStr>,
    ) -> Result<Self, LinkerDiscoveryError> {
        Self::discover_in(
            search_path.as_ref(),
            target.linker_candidates(),
            target.linker_flavor(),
        )
    }

    fn discover_in(
        search_path: &OsStr,
        candidates: &[&str],
        flavor: LinkerFlavor,
    ) -> Result<Self, LinkerDiscoveryError> {
        let directories = env::split_paths(search_path)
            .filter(|directory| !directory.as_os_str().is_empty())
            .collect::<Vec<_>>();

        for candidate in candidates {
            for directory in &directories {
                let program = directory.join(candidate);
                if is_executable_file(&program) {
                    return Ok(Self::new_with_flavor(program.into_os_string(), flavor));
                }
            }
        }

        Err(LinkerDiscoveryError::NotFound {
            candidates: candidates.iter().map(OsString::from).collect(),
            search_path: search_path.to_os_string(),
            flavor,
        })
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
        Self::validate_output_path(request)?;
        Self::execute(request, command)
    }

    /// Exclusively creates `response_file`, retains it after the linker exits,
    /// and invokes the linker with the generated `@file` argument. An existing
    /// path (including a symlink) is rejected without modifying its contents.
    pub fn link_with_response_file(
        &self,
        request: &LinkRequest,
        response_file: impl AsRef<Path>,
    ) -> Result<(), LinkError> {
        let response_file = response_file.as_ref();
        let command = self.command_with_response_file(request, response_file)?;
        Self::validate_output_path(request)?;
        let contents = self.response_file_contents(request)?;
        // Exclusive creation also rejects symlinks and differently spelled
        // aliases of an existing input. Never truncate a caller's file.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(response_file)
            .and_then(|mut file| file.write_all(&contents))
            .map_err(|source| LinkError::WriteResponseFile {
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

    // Command preview remains independent of the filesystem. Only execution
    // performs path preflight, before creating an rsp file or starting a tool.
    fn validate_output_path(request: &LinkRequest) -> Result<(), LinkError> {
        let parent = request
            .output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let directory = fs::metadata(parent).map_err(|source| LinkError::OutputDirectory {
            path: parent.to_path_buf(),
            source,
        })?;
        if !directory.is_dir() {
            return Err(LinkError::OutputDirectory {
                path: parent.to_path_buf(),
                source: io::ErrorKind::NotADirectory.into(),
            });
        }
        match fs::metadata(&request.output) {
            Ok(metadata) if !metadata.is_file() => Err(LinkError::OutputNotFile {
                path: request.output.clone(),
            }),
            Ok(_) => Ok(()),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(LinkError::InspectOutput {
                path: request.output.clone(),
                source,
            }),
        }
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
                .map(|object| literal_path_argument(object)),
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
                    argument = literal_path_argument(Path::new(&argument));
                }
            }
            arguments.push(argument);
        }
        match self.flavor {
            LinkerFlavor::Cc => {
                arguments.push(OsString::from("-o"));
                arguments.push(literal_path_argument(&request.output));
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
                output: request.output.clone(),
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

// Drivers expand @response files even where a filename is expected, and leading
// '-' input paths are options. Preserve relative filenames without changing cwd
// or requiring the output file to exist. Both command forms share this boundary.
fn literal_path_argument(path: &Path) -> OsString {
    if path.is_relative()
        && matches!(
            path.as_os_str().as_encoded_bytes().first(),
            Some(b'-' | b'@')
        )
    {
        Path::new(".").join(path).into_os_string()
    } else {
        path.as_os_str().to_os_string()
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

fn host_linker_candidates() -> (&'static [&'static str], LinkerFlavor) {
    if let Some(target) = LinkTarget::host() {
        (target.linker_candidates(), target.linker_flavor())
    } else if cfg!(target_env = "msvc") {
        (&["link.exe"], LinkerFlavor::MsvcLink)
    } else if cfg!(windows) {
        (&["cc.exe", "clang.exe", "gcc.exe"], LinkerFlavor::Cc)
    } else {
        (&["cc", "clang", "gcc"], LinkerFlavor::Cc)
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[derive(Debug)]
pub enum LinkError {
    OutputDirectory {
        path: PathBuf,
        source: io::Error,
    },
    OutputNotFile {
        path: PathBuf,
    },
    InspectOutput {
        path: PathBuf,
        source: io::Error,
    },
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
        output: PathBuf,
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
            Self::OutputDirectory { path, source } => {
                write!(
                    formatter,
                    "cannot use link output directory `{}`: {source}; ",
                    path.display()
                )?;
                formatter.write_str(match source.kind() {
                    io::ErrorKind::NotFound => "create the output directory before linking",
                    io::ErrorKind::NotADirectory => {
                        "choose an existing directory for the output file"
                    }
                    io::ErrorKind::PermissionDenied => {
                        "check access permissions for the output directory"
                    }
                    _ => "check that the output directory exists and is accessible",
                })
            }
            Self::OutputNotFile { path } => write!(
                formatter,
                "link output `{}` is not a regular file; choose an output file in an existing directory",
                path.display()
            ),
            Self::InspectOutput { path, source } => write!(
                formatter,
                "cannot inspect link output `{}`: {source}; check the output path and permissions",
                path.display()
            ),
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
                )?;
                formatter.write_str(match source.kind() {
                    io::ErrorKind::NotFound => "; configure the target toolchain on PATH or select its full executable path",
                    io::ErrorKind::PermissionDenied => "; check executable permissions and access to the linker",
                    _ => "",
                })
            }
            Self::Failed {
                command,
                output,
                status,
                stdout,
                stderr,
            } => write!(
                formatter,
                "system linker command {command} failed for output `{}` with {status}:\nstdout:\n{}\nstderr:\n{}",
                output.display(),
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
            Self::OutputDirectory { source, .. }
            | Self::InspectOutput { source, .. }
            | Self::WriteResponseFile { source, .. }
            | Self::Spawn { source, .. } => Some(source),
            Self::OutputNotFile { .. } => None,
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
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        LinkError, LinkRequest, LinkTarget, LinkerDiscoveryError, LinkerFlavor, SystemLinker,
        UnsupportedLinkTarget, host_linker_candidates,
    };

    const MISSING_LINKER: &str = "lang-linker-test-command-that-does-not-exist";
    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn create() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos();
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "lang linker discovery test {} {nonce} {sequence}",
                std::process::id(),
            ));
            fs::create_dir(&path).expect("creates an isolated linker discovery directory");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn create_executable(path: &Path) {
        fs::write(path, b"").expect("creates a linker candidate");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(path)
                .expect("reads linker candidate metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).expect("makes linker candidate executable");
        }
    }

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
    fn option_like_filenames_remain_literal_in_commands_and_response_files() {
        let request = LinkRequest::new("@output")
            .with_object("-input.o")
            .with_object("@input.o");
        for flavor in [LinkerFlavor::Cc, LinkerFlavor::MsvcLink] {
            let linker = SystemLinker::new_with_flavor("unused", flavor);
            let command = linker.command(&request).unwrap();
            assert_eq!(command.arguments()[0], Path::new(".").join("-input.o"));
            assert_eq!(command.arguments()[1], Path::new(".").join("@input.o"));
            let bytes = linker.response_file_contents(&request).unwrap();
            let response = match flavor {
                LinkerFlavor::Cc => String::from_utf8(bytes).unwrap(),
                LinkerFlavor::MsvcLink => String::from_utf16(
                    &bytes[2..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| u16::from_le_bytes([b[0], b[1]]))
                        .collect::<Vec<_>>(),
                )
                .unwrap(),
            };
            assert!(!response.lines().any(|line| line.starts_with("\"@")));
            assert!(!response.lines().any(|line| line.starts_with("\"-input")));
        }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn cc_links_option_like_relative_filenames_directly_and_through_response_file() {
        let directory = TestDirectory::create();
        fs::write(
            directory.path().join("main.c"),
            "int main(void) { return 0; }",
        )
        .unwrap();
        assert!(
            std::process::Command::new("cc")
                .current_dir(directory.path())
                .args(["-c", "main.c", "-o", "./-input.o"])
                .status()
                .unwrap()
                .success()
        );
        fs::copy(
            directory.path().join("-input.o"),
            directory.path().join("@input.o"),
        )
        .unwrap();
        let linker = SystemLinker::new("cc");
        for input in ["-input.o", "@input.o"] {
            let request = LinkRequest::new("@output").with_object(input);
            fs::write(
                directory.path().join("args.rsp"),
                linker.response_file_contents(&request).unwrap(),
            )
            .unwrap();
            for command in [
                linker.command(&request).unwrap(),
                linker
                    .command_with_response_file(&request, "args.rsp")
                    .unwrap(),
            ] {
                assert!(
                    std::process::Command::new(command.program())
                        .current_dir(directory.path())
                        .args(command.arguments())
                        .status()
                        .unwrap()
                        .success()
                );
                assert!(
                    std::process::Command::new(directory.path().join("@output"))
                        .status()
                        .unwrap()
                        .success()
                );
                fs::remove_file(directory.path().join("@output")).unwrap();
            }
        }
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
    fn preserves_existing_response_files_and_aliased_inputs() {
        let directory = TestDirectory::create();
        let input = directory.path().join("input.o");
        fs::write(&input, b"object contents").unwrap();
        let request = LinkRequest::new(directory.path().join("program")).with_object(&input);
        let alias = directory.path().join(".").join("input.o");
        let error = SystemLinker::new(MISSING_LINKER)
            .link_with_response_file(&request, alias)
            .unwrap_err();
        assert!(matches!(
            error,
            LinkError::WriteResponseFile { .. } | LinkError::ResponseFileConflictsWithObject { .. }
        ));
        assert_eq!(fs::read(&input).unwrap(), b"object contents");

        let response = directory.path().join("existing.rsp");
        fs::write(&response, b"keep response").unwrap();
        assert!(matches!(
            SystemLinker::new(MISSING_LINKER).link_with_response_file(&request, &response),
            Err(LinkError::WriteResponseFile { .. })
        ));
        assert_eq!(fs::read(response).unwrap(), b"keep response");
    }

    #[cfg(unix)]
    #[test]
    fn refuses_response_file_symlinks() {
        let directory = TestDirectory::create();
        let input = directory.path().join("input.o");
        fs::write(&input, b"object contents").unwrap();
        let response = directory.path().join("alias.rsp");
        std::os::unix::fs::symlink(&input, &response).unwrap();
        let request = LinkRequest::new(directory.path().join("program")).with_object(&input);
        assert!(matches!(
            SystemLinker::new(MISSING_LINKER).link_with_response_file(&request, response),
            Err(LinkError::WriteResponseFile { .. })
        ));
        assert_eq!(fs::read(input).unwrap(), b"object contents");
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
    fn tier_one_link_targets_round_trip_through_their_triples() {
        for target in [LinkTarget::MacosAarch64, LinkTarget::WindowsX86_64Msvc] {
            assert_eq!(target.triple().parse::<LinkTarget>(), Ok(target));
        }
    }

    #[test]
    fn unsupported_link_target_preserves_the_triple() {
        let error = "x86_64-unknown-linux-gnu"
            .parse::<LinkTarget>()
            .expect_err("rejects a target outside the initial tier-one policy");

        assert_eq!(error.triple(), "x86_64-unknown-linux-gnu");
        assert_eq!(
            error,
            UnsupportedLinkTarget {
                triple: String::from("x86_64-unknown-linux-gnu"),
            }
        );
    }

    #[test]
    fn tier_one_targets_select_their_linker_policy() {
        let macos = SystemLinker::for_target(LinkTarget::MacosAarch64);
        assert_eq!(macos.program(), OsStr::new("cc"));
        assert_eq!(macos.flavor(), LinkerFlavor::Cc);

        let windows = SystemLinker::for_target(LinkTarget::WindowsX86_64Msvc);
        assert_eq!(windows.program(), OsStr::new("link.exe"));
        assert_eq!(windows.flavor(), LinkerFlavor::MsvcLink);
    }

    #[test]
    fn discovers_a_linker_for_an_explicit_tier_one_target() {
        let directory = TestDirectory::create();
        let expected_program = directory.path().join("link.exe");
        create_executable(&expected_program);
        let search_path = std::env::join_paths([directory.path()]).unwrap();

        let linker =
            SystemLinker::discover_target_in(LinkTarget::WindowsX86_64Msvc, &search_path).unwrap();

        assert_eq!(linker.program(), expected_program.as_os_str());
        assert_eq!(linker.flavor(), LinkerFlavor::MsvcLink);
    }

    #[test]
    fn discovers_the_preferred_host_linker_on_path() {
        let directory = TestDirectory::create();
        let (candidates, expected_flavor) = host_linker_candidates();
        let expected_program = directory.path().join(candidates[0]);
        create_executable(&expected_program);
        let search_path = std::env::join_paths([directory.path()]).unwrap();

        let linker = SystemLinker::discover_host_in(&search_path).unwrap();

        assert_eq!(linker.program(), expected_program.as_os_str());
        assert_eq!(linker.flavor(), expected_flavor);
    }

    #[cfg(all(unix, not(target_env = "msvc")))]
    #[test]
    fn skips_non_executable_candidates_during_discovery() {
        let directory = TestDirectory::create();
        fs::write(directory.path().join("cc"), b"").expect("creates a non-executable candidate");
        let expected_program = directory.path().join("clang");
        create_executable(&expected_program);
        let search_path = std::env::join_paths([directory.path()]).unwrap();

        let linker = SystemLinker::discover_host_in(&search_path).unwrap();

        assert_eq!(linker.program(), expected_program.as_os_str());
    }

    #[cfg(not(target_env = "msvc"))]
    #[test]
    fn discovery_prefers_candidate_order_before_path_order() {
        let first_directory = TestDirectory::create();
        let second_directory = TestDirectory::create();
        create_executable(&first_directory.path().join(if cfg!(windows) {
            "clang.exe"
        } else {
            "clang"
        }));
        let expected_program =
            second_directory
                .path()
                .join(if cfg!(windows) { "cc.exe" } else { "cc" });
        create_executable(&expected_program);
        let search_path =
            std::env::join_paths([first_directory.path(), second_directory.path()]).unwrap();

        let linker = SystemLinker::discover_host_in(&search_path).unwrap();

        assert_eq!(linker.program(), expected_program.as_os_str());
    }

    #[test]
    fn reports_searched_candidates_when_discovery_fails() {
        let directory = TestDirectory::create();
        let search_path = std::env::join_paths([directory.path()]).unwrap();
        let (expected_candidates, flavor) = host_linker_candidates();

        let error = SystemLinker::discover_host_in(&search_path)
            .expect_err("reports missing host linker candidates");

        assert_eq!(
            error,
            LinkerDiscoveryError::NotFound {
                candidates: expected_candidates.iter().map(OsString::from).collect(),
                search_path,
                flavor,
            }
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

    #[test]
    fn output_path_errors_precede_spawning_and_response_file_creation() {
        use std::io::ErrorKind;
        let directory = TestDirectory::create();
        let input = directory.path().join("main.o");
        fs::write(&input, b"preserve input").unwrap();
        let not_directory = directory.path().join("parent-file");
        fs::write(&not_directory, b"preserve parent").unwrap();
        let existing_directory = directory.path().join("output-directory");
        fs::create_dir(&existing_directory).unwrap();
        for flavor in [LinkerFlavor::Cc, LinkerFlavor::MsvcLink] {
            for output in [
                directory.path().join("missing/program"),
                not_directory.join("program"),
                existing_directory.clone(),
            ] {
                let request = LinkRequest::new(&output).with_object(&input);
                let linker = SystemLinker::new_with_flavor(MISSING_LINKER, flavor);
                // Command previews need no files or tools and remain useful for
                // cross-target build planning; execution performs the preflight.
                assert!(linker.command(&request).is_ok());
                let response = directory.path().join("unused.rsp");
                for error in [
                    linker.link(&request).unwrap_err(),
                    linker
                        .link_with_response_file(&request, &response)
                        .unwrap_err(),
                ] {
                    let message = error.to_string();
                    if output == existing_directory {
                        assert!(
                            matches!(error, LinkError::OutputNotFile { ref path } if path == &output)
                        );
                        assert!(message.contains("choose an output file"));
                    } else {
                        let expected = if output.starts_with(&not_directory) {
                            ErrorKind::NotADirectory
                        } else {
                            ErrorKind::NotFound
                        };
                        assert!(
                            matches!(error, LinkError::OutputDirectory { ref path, ref source } if Some(path.as_path()) == output.parent() && source.kind() == expected)
                        );
                        let cause = std::error::Error::source(&error)
                            .unwrap()
                            .downcast_ref::<std::io::Error>()
                            .unwrap();
                        assert_eq!(cause.kind(), expected);
                        assert!(message.contains(if expected == ErrorKind::NotFound {
                            "create the output directory"
                        } else {
                            "choose an existing directory"
                        }));
                    }
                    assert!(
                        !response.exists(),
                        "invalid output must not leave a response file"
                    );
                    assert_eq!(fs::read(&input).unwrap(), b"preserve input");
                    assert_eq!(fs::read(&not_directory).unwrap(), b"preserve parent");
                }
            }
        }
    }

    #[test]
    fn discovery_reports_the_requested_toolchain_and_setup_action() {
        let directory = TestDirectory::create();
        let search_path = std::env::join_paths([directory.path()]).unwrap();
        for (target, hint) in [
            (LinkTarget::MacosAarch64, "Xcode Command Line Tools"),
            (
                LinkTarget::WindowsX86_64Msvc,
                "x64 Developer Command Prompt",
            ),
        ] {
            let error = SystemLinker::discover_target_in(target, &search_path).unwrap_err();
            let message = error.to_string();
            assert!(
                matches!(error, LinkerDiscoveryError::NotFound { flavor, ref candidates, ref search_path } if flavor == target.linker_flavor() && candidates.iter().any(|c| c == target.default_linker_program()) && !search_path.is_empty())
            );
            assert!(message.contains(hint), "{message}");
            assert!(
                message.contains(target.default_linker_program()),
                "{message}"
            );
        }
        let request = LinkRequest::new(directory.path().join("program")).with_object("main.o");
        let error = SystemLinker::new(MISSING_LINKER)
            .link(&request)
            .unwrap_err();
        assert!(matches!(error, LinkError::Spawn { .. }));
        assert!(error.to_string().contains("full executable path"));
        assert_eq!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
}
