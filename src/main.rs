use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;
use sysml::{
    check_paths, validate_paths, CheckReport, ValidationProfileId, ValidationReport,
    REQUIREMENTS_STRUCTURE_PROFILE_ID,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Human,
    Json,
}

const VALIDATE_COMMAND: &str = "validate";

fn main() -> ExitCode {
    run(env::args_os().collect())
}

fn run(arguments: Vec<OsString>) -> ExitCode {
    let program = arguments
        .first()
        .cloned()
        .unwrap_or_else(|| OsString::from("sysml"));
    let Some(command) = arguments.get(1) else {
        print_usage(&program);
        return ExitCode::from(2);
    };

    match command.to_string_lossy().as_ref() {
        "-h" | "--help" => {
            print_help(&program);
            ExitCode::SUCCESS
        }
        "-V" | "--version" => {
            println!("sysml {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "check" => run_check(&program, &arguments[2..]),
        VALIDATE_COMMAND => run_validate(&program, &arguments[2..]),
        _ => {
            eprintln!("error: unknown command {}", command.to_string_lossy());
            print_usage(&program);
            ExitCode::from(2)
        }
    }
}

/// Run one explicitly selected validation profile without changing `check`.
fn run_validate(program: &OsString, arguments: &[OsString]) -> ExitCode {
    if arguments.len() == 1 && matches!(arguments[0].to_string_lossy().as_ref(), "-h" | "--help") {
        print_validate_help(program);
        return ExitCode::SUCCESS;
    }

    let (format, profile, paths) = match parse_validate_arguments(arguments) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("error: {message}");
            print_validate_usage(program);
            return ExitCode::from(2);
        }
    };

    let profile_id = match profile.as_str() {
        REQUIREMENTS_STRUCTURE_PROFILE_ID => ValidationProfileId::RequirementsStructureV1,
        _ => {
            eprintln!(
                "error: unsupported validation profile {profile:?}; expected {REQUIREMENTS_STRUCTURE_PROFILE_ID}"
            );
            return ExitCode::from(2);
        }
    };

    match validate_paths(&paths, profile_id) {
        Ok(report) => emit_validation_report(&report, format),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn run_check(program: &OsString, arguments: &[OsString]) -> ExitCode {
    if arguments.len() == 1 && matches!(arguments[0].to_string_lossy().as_ref(), "-h" | "--help") {
        print_check_help(program);
        return ExitCode::SUCCESS;
    }

    let (format, paths) = match parse_check_arguments(arguments) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("error: {message}");
            print_check_usage(program);
            return ExitCode::from(2);
        }
    };

    match check_paths(&paths) {
        Ok(report) => emit_check_report(&report, format),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn parse_check_arguments(arguments: &[OsString]) -> Result<(OutputFormat, Vec<PathBuf>), String> {
    let mut format = OutputFormat::Human;
    let mut paths = Vec::new();
    let mut index = 0;

    while index < arguments.len() {
        let argument = arguments[index].to_string_lossy();
        match argument.as_ref() {
            "--" => {
                paths.extend(arguments[index + 1..].iter().map(PathBuf::from));
                break;
            }
            "-h" | "--help" => {
                return Err("--help must be used without model paths".to_owned());
            }
            "--format" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--format requires human or json".to_owned())?;
                format = parse_output_format(&value.to_string_lossy())?;
            }
            value if value.starts_with("--format=") => {
                format = parse_output_format(&value["--format=".len()..])?;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option {value}"));
            }
            _ => paths.push(PathBuf::from(&arguments[index])),
        }
        index += 1;
    }

    if paths.is_empty() {
        return Err("at least one SysML file or directory is required".to_owned());
    }
    Ok((format, paths))
}

fn parse_validate_arguments(
    arguments: &[OsString],
) -> Result<(OutputFormat, String, Vec<PathBuf>), String> {
    let mut format = OutputFormat::Human;
    let mut profile = None;
    let mut paths = Vec::new();
    let mut index = 0;

    while index < arguments.len() {
        let argument = arguments[index].to_string_lossy();
        match argument.as_ref() {
            "--" => {
                paths.extend(arguments[index + 1..].iter().map(PathBuf::from));
                break;
            }
            "-h" | "--help" => {
                return Err("--help must be used without model paths".to_owned());
            }
            "--format" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--format requires human or json".to_owned())?;
                format = parse_output_format(&value.to_string_lossy())?;
            }
            value if value.starts_with("--format=") => {
                format = parse_output_format(&value["--format=".len()..])?;
            }
            "--profile" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--profile requires a profile identifier".to_owned())?;
                profile = Some(value.to_string_lossy().into_owned());
            }
            value if value.starts_with("--profile=") => {
                profile = Some(value["--profile=".len()..].to_owned());
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option {value}"));
            }
            _ => paths.push(PathBuf::from(&arguments[index])),
        }
        index += 1;
    }

    let profile = profile.ok_or_else(|| "--profile is required".to_owned())?;
    if paths.is_empty() {
        return Err("at least one SysML file or directory is required".to_owned());
    }
    Ok((format, profile, paths))
}

fn parse_output_format(value: &str) -> Result<OutputFormat, String> {
    match value {
        "human" => Ok(OutputFormat::Human),
        "json" => Ok(OutputFormat::Json),
        _ => Err(format!(
            "unsupported output format {value:?}; expected human or json"
        )),
    }
}

fn emit_check_report(report: &CheckReport, format: OutputFormat) -> ExitCode {
    match format {
        OutputFormat::Json => match serde_json::to_string_pretty(report) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("error: failed to serialize check report: {error}");
                return ExitCode::from(2);
            }
        },
        OutputFormat::Human => {
            for file in &report.files {
                for diagnostic in &file.diagnostics {
                    eprintln!(
                        "{}:{}:{}: {}[{}]: {}",
                        file.path,
                        diagnostic.span.start_line,
                        diagnostic.span.start_column,
                        diagnostic.severity,
                        diagnostic.code,
                        diagnostic.message
                    );
                }
            }
            let noun = if report.files.len() == 1 {
                "file"
            } else {
                "files"
            };
            if report.valid {
                println!("checked {} SysML {noun}: syntax valid", report.files.len());
            } else {
                eprintln!(
                    "checked {} SysML {noun}: syntax errors found",
                    report.files.len()
                );
            }
        }
    }

    if report.valid {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn emit_validation_report(report: &ValidationReport, format: OutputFormat) -> ExitCode {
    match format {
        OutputFormat::Json => match serde_json::to_string_pretty(report) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("error: failed to serialize validation report: {error}");
                return ExitCode::from(2);
            }
        },
        OutputFormat::Human => {
            for file in &report.files {
                for diagnostic in &file.diagnostics {
                    eprintln!(
                        "{}:{}:{}: {}[{}]: {}",
                        file.path,
                        diagnostic.span.start_line,
                        diagnostic.span.start_column,
                        diagnostic.severity,
                        diagnostic.code,
                        diagnostic.message
                    );
                }
            }
            let noun = if report.files.len() == 1 {
                "file"
            } else {
                "files"
            };
            if report.valid {
                println!(
                    "validated {} SysML {noun}: {}",
                    report.files.len(),
                    report.profile.id
                );
            } else {
                eprintln!(
                    "validated {} SysML {noun}: {} errors found",
                    report.files.len(),
                    report.profile.id
                );
            }
        }
    }

    if report.valid {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print_usage(program: &OsString) {
    eprintln!(
        "usage: {} check [--format human|json] <path>...\n       {} validate --profile <id> [--format human|json] <path>...",
        program.to_string_lossy(),
        program.to_string_lossy(),
    );
}

fn print_check_usage(program: &OsString) {
    eprintln!(
        "usage: {} check [--format human|json] <path>...",
        program.to_string_lossy()
    );
}

fn print_validate_usage(program: &OsString) {
    eprintln!(
        "usage: {} validate --profile <id> [--format human|json] <path>...",
        program.to_string_lossy()
    );
}

fn print_check_help(program: &OsString) {
    println!(
        "Check SysML textual syntax\n\n\
         Usage:\n  {} check [--format human|json] <path>...\n\n\
         Paths may be .sysml files or directories, which are searched recursively.\n\
         Version 1 reports syntax diagnostics only and does not claim full semantic\n\
         or OMG SysML v2 conformance.",
        program.to_string_lossy()
    );
}

fn print_validate_help(program: &OsString) {
    println!(
        "Validate SysML against a documented semantic profile\n\n\
         Usage:\n  {} validate --profile {} [--format human|json] <path>...\n\n\
         Paths may be .sysml files or directories, which are searched recursively.\n\
         The requirements structure profile implements a bounded SysML 2.0 rule set;\n\
         it does not claim complete semantic or OMG conformance.",
        program.to_string_lossy(),
        REQUIREMENTS_STRUCTURE_PROFILE_ID,
    );
}

fn print_help(program: &OsString) {
    println!(
        "{} - headless SysML v2 model tooling\n\n\
         Usage:\n  {} check [--format human|json] <path>...\n  {} validate --profile <id> [--format human|json] <path>...\n\n\
         Commands:\n  check       Check .sysml syntax in files or directories\n  validate    Validate .sysml against an explicit bounded profile\n\n\
         The check command reports syntax coverage only; it does not claim full\n\
         OMG SysML v2 semantic conformance.",
        program.to_string_lossy(),
        program.to_string_lossy(),
        program.to_string_lossy()
    );
}
