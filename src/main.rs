use std::process::ExitCode;

use lang::run_prototype_add;

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
    let mut arguments = std::env::args().skip(1);
    let left = parse_argument(arguments.next(), "left")?;
    let right = parse_argument(arguments.next(), "right")?;

    if arguments.next().is_some() {
        return Err("usage: cargo run -- <left> <right>".to_owned());
    }

    let results = run_prototype_add(left, right).map_err(|error| error.to_string())?;
    println!("bytecode: {}", results.bytecode);
    println!("native:   {}", results.native);

    if results.bytecode != results.native {
        return Err("execution modes produced different results".to_owned());
    }

    Ok(())
}

fn parse_argument(value: Option<String>, name: &str) -> Result<i64, String> {
    let value =
        value.ok_or_else(|| format!("missing {name}; usage: cargo run -- <left> <right>"))?;
    value
        .parse()
        .map_err(|error| format!("invalid {name} value {value:?}: {error}"))
}
