use std::path::PathBuf;
use std::process::ExitCode;

use lang::{prototype_add_function, prototype_host_callback_function, prototype_min_function};
use lang_codegen_clif::emit_host_object;
use lang_mir::Function;

const USAGE: &str = "usage: cargo run --release --bin prototype-object -- \
                     <add|min|host-callback> <output.o>";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Prototype {
    Add,
    Min,
    HostCallback,
}

impl Prototype {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "add" => Ok(Self::Add),
            "min" => Ok(Self::Min),
            "host-callback" => Ok(Self::HostCallback),
            _ => Err(format!("unknown prototype {value:?}; {USAGE}")),
        }
    }

    fn function(self) -> Function {
        match self {
            Self::Add => prototype_add_function(),
            Self::Min => prototype_min_function(),
            Self::HostCallback => prototype_host_callback_function(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Arguments {
    prototype: Prototype,
    output: PathBuf,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let arguments = Arguments::parse(std::env::args().skip(1))?;
    let function = arguments.prototype.function();
    let artifact = emit_host_object(&function).map_err(|error| error.to_string())?;
    std::fs::write(&arguments.output, artifact.bytes()).map_err(|error| {
        format!(
            "failed to write object {}: {error}",
            arguments.output.display()
        )
    })?;

    println!("object:   {}", arguments.output.display());
    println!("symbol:   {}", artifact.function_symbol());
    println!("code:     {} bytes", artifact.code_size());
    println!("artifact: {} bytes", artifact.bytes().len());
    Ok(())
}

impl Arguments {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut arguments = arguments.into_iter();
        let prototype = Prototype::parse(
            &arguments
                .next()
                .ok_or_else(|| format!("missing prototype; {USAGE}"))?,
        )?;
        let output = arguments
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| format!("missing output path; {USAGE}"))?;
        if arguments.next().is_some() {
            return Err(format!("too many arguments; {USAGE}"));
        }
        Ok(Self { prototype, output })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Arguments, Prototype, USAGE};

    #[test]
    fn parses_supported_prototypes() {
        assert_eq!(
            Arguments::parse(["add".to_owned(), "add.o".to_owned()]),
            Ok(Arguments {
                prototype: Prototype::Add,
                output: PathBuf::from("add.o"),
            })
        );
        assert_eq!(
            Arguments::parse(["min".to_owned(), "min.o".to_owned()]),
            Ok(Arguments {
                prototype: Prototype::Min,
                output: PathBuf::from("min.o"),
            })
        );
        assert_eq!(
            Arguments::parse(["host-callback".to_owned(), "callback.o".to_owned()]),
            Ok(Arguments {
                prototype: Prototype::HostCallback,
                output: PathBuf::from("callback.o"),
            })
        );
    }

    #[test]
    fn rejects_missing_and_extra_arguments() {
        assert_eq!(
            Arguments::parse([]),
            Err(format!("missing prototype; {USAGE}"))
        );
        assert_eq!(
            Arguments::parse(["add".to_owned()]),
            Err(format!("missing output path; {USAGE}"))
        );
        assert_eq!(
            Arguments::parse(["add".to_owned(), "add.o".to_owned(), "extra".to_owned()]),
            Err(format!("too many arguments; {USAGE}"))
        );
    }
}
