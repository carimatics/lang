//! RFC 0043: source editions and inspectable, independent compatibility domains.
use super::Diagnostic;
use serde::Serialize;
use std::ffi::OsString;
use std::io::{self, Write};

pub const USAGE: &str = "       lang compatibility [--message-format human|json]\n";

/// An edition selects source semantics; it is not a compiler or artifact version.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum SourceEdition {
    #[serde(rename = "2026")]
    E2026,
}

impl SourceEdition {
    pub const CURRENT: Self = Self::E2026;
    pub const SUPPORTED: &[Self] = &[Self::E2026];

    pub fn parse(value: &str) -> Result<Self, Diagnostic> {
        match value {
            "2026" => Ok(Self::E2026),
            _ => Err(Diagnostic::error(
                "PKG0002",
                "unsupported package edition; expected 2026",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::E2026 => "2026",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Experimental,
    Stabilized,
    Unavailable,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Experimental => "experimental",
            Self::Stabilized => "stabilized",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Domain {
    pub domain: &'static str,
    pub status: Status,
    pub policy: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub compiler_version: &'static str,
    pub source_editions: &'static [SourceEdition],
    pub default_source_edition: SourceEdition,
    pub domains: [Domain; 7],
}

impl Report {
    /// Describes this compiler without inspecting the project or granting authority.
    pub fn current() -> Self {
        use Status::{Experimental, Stabilized, Unavailable};
        Self {
            schema: 1,
            compiler_version: env!("CARGO_PKG_VERSION"),
            source_editions: SourceEdition::SUPPORTED,
            default_source_edition: SourceEdition::CURRENT,
            domains: [
                Domain {
                    domain: "source",
                    status: Experimental,
                    policy: "Explicit per-package edition; standalone/REPL use 2026. Unknown editions are refused; no implicit migration or end-of-support date.",
                },
                Domain {
                    domain: "package",
                    status: Experimental,
                    policy: "Manifest schema 1; lock schema 1/2 with exact target, toolchain and input checksums. Unsupported schemas are refused; regenerate locks explicitly.",
                },
                Domain {
                    domain: "bytecode",
                    status: Experimental,
                    policy: "LANGBC schemas 1-5 have separate profile verifiers. Unsupported/incompatible artifacts require rebuilding from source.",
                },
                Domain {
                    domain: "native-abi",
                    status: Experimental,
                    policy: "Private Rust/native linkage requires the matching target/build and SDK profile. Rebuild native packages; version equality grants no trust or authority.",
                },
                Domain {
                    domain: "component-abi",
                    status: Stabilized,
                    policy: "Only public C v1 is stabilized (RFC 0033): preserve old callers and generated clients. IDL, wire formats and private SDK remain separately versioned; negotiate contracts before effects.",
                },
                Domain {
                    domain: "tooling",
                    status: Experimental,
                    policy: "Package diagnostics schema 1 and explicit exit codes; editor/debug/profile/distribution contracts are independently versioned. Incompatible shapes require schema decisions.",
                },
                Domain {
                    domain: "bootstrap",
                    status: Unavailable,
                    policy: "No supported self-hosting chain; M6 must freeze its subset and verify reproducible stages.",
                },
            ],
        }
    }
}

pub fn run(
    arguments: &[OsString],
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<u8> {
    let json_requested = arguments
        .windows(2)
        .any(|pair| pair[0] == "--message-format" && pair[1] == "json");
    let json = match arguments {
        [] => false,
        [option, format] if option == "--message-format" && format == "human" => false,
        [option, format] if option == "--message-format" && format == "json" => true,
        _ => {
            let diagnostic = Diagnostic::error(
                "CLI0001",
                "compatibility accepts only --message-format human|json",
            );
            if json_requested {
                serde_json::to_writer(&mut *stderr, &diagnostic)?;
                writeln!(stderr)?;
            } else {
                writeln!(stderr, "error: {diagnostic}\n{USAGE}")?;
            }
            return Ok(2);
        }
    };
    let report = Report::current();
    if json {
        serde_json::to_writer(&mut *stdout, &report)?;
        writeln!(stdout)?;
    } else {
        writeln!(
            stdout,
            "lang {} compatibility (schema 1)",
            report.compiler_version
        )?;
        write!(stdout, "Source editions:")?;
        for edition in report.source_editions {
            write!(stdout, " {}", edition.as_str())?;
        }
        writeln!(
            stdout,
            "; standalone/REPL default: {}",
            report.default_source_edition.as_str()
        )?;
        for domain in report.domains {
            writeln!(
                stdout,
                "{} [{}]: {}",
                domain.domain,
                domain.status.as_str(),
                domain.policy
            )?;
        }
    }
    Ok(0)
}
