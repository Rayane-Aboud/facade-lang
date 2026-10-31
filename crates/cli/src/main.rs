use std::env;
use std::fs;
use std::process::ExitCode;

const USAGE: &str = "usage: facadec check <file.fcd>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let path = match args.as_slice() {
        [cmd, path] if cmd == "check" => path,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("error: cannot read `{path}`: {err}");
            return ExitCode::from(2);
        }
    };

    let diagnostics = semantics::check(&source);
    if diagnostics.is_empty() {
        println!("ok: {path}");
        ExitCode::SUCCESS
    } else {
        for d in &diagnostics {
            eprintln!("{path}: {d}");
        }
        ExitCode::from(1)
    }
}
