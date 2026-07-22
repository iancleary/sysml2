use std::env;
use std::process::ExitCode;
use sysml::Model;

fn main() -> ExitCode {
    let mut arguments = env::args_os();
    let program = arguments.next().unwrap_or_else(|| "sysml".into());
    let Some(path) = arguments.next() else {
        eprintln!("usage: {} <model.sysml.toml>", program.to_string_lossy());
        return ExitCode::from(2);
    };
    if arguments.next().is_some() {
        eprintln!("error: expected exactly one model path");
        return ExitCode::from(2);
    }

    match Model::load(&path) {
        Ok(model) => {
            let summary = model.summary();
            println!(
                "{}: {} elements ({} definitions, {} usages), {} relationships",
                model.name,
                summary.elements,
                summary.definitions,
                summary.usages,
                summary.relationships
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
