use korri::{client::Client, error::CliError, module::Context, target};
use std::{path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args_os().collect();
    let json = arguments.iter().any(|arg| arg == "--json");
    let matches = match korri::command().try_get_matches_from(arguments) {
        Ok(matches) => matches,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            let _ = error.print();
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            let cause = match error.kind() {
                clap::error::ErrorKind::InvalidSubcommand => "Unknown command.",
                clap::error::ErrorKind::UnknownArgument => "Unknown argument.",
                clap::error::ErrorKind::MissingRequiredArgument => "Required input is missing.",
                clap::error::ErrorKind::InvalidValue => "An argument has an invalid value.",
                _ => "Invalid command or arguments.",
            };
            CliError::usage(cause).print(json);
            return ExitCode::from(2);
        }
    };
    match run(&matches) {
        Ok(exit) => ExitCode::from(exit),
        Err(error) => {
            error.print(json);
            ExitCode::from(error.exit)
        }
    }
}

fn run(matches: &clap::ArgMatches) -> Result<u8, CliError> {
    let json = matches.get_flag("json");
    match matches.subcommand() {
        Some(("version", _)) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&*korri::VERSION).expect("serialize version")
                );
            } else {
                println!("korri {}", *korri::VERSION);
            }
            return Ok(0);
        }
        Some(("completion", args)) => {
            let shell = *args
                .get_one::<clap_complete::Shell>("shell")
                .expect("required argument");
            if json {
                let mut script = Vec::new();
                clap_complete::generate(shell, &mut korri::command(), "korri", &mut script);
                let script = String::from_utf8(script)
                    .map_err(|_| CliError::usage("Could not encode shell completion."))?;
                println!(
                    "{}",
                    serde_json::to_string(&script).expect("serialize completion")
                );
            } else {
                clap_complete::generate(
                    shell,
                    &mut korri::command(),
                    "korri",
                    &mut std::io::stdout(),
                );
            }
            return Ok(0);
        }
        _ => {}
    }
    let socket = matches
        .get_one::<String>("socket")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("KORRI_CLI_SOCKET").map(PathBuf::from))
        .unwrap_or_else(|| korrid::cli::DEFAULT_SOCKET.into());
    if !socket.is_absolute() {
        return Err(CliError::usage("The local socket path must be absolute."));
    }
    let client = Client::new(&socket)?;
    let selector = matches.get_one::<String>("device");
    let device = selector
        .map(|selector| target::read(&client).and_then(|facts| target::resolve(&facts, selector)))
        .transpose()?
        .flatten();
    let context = Context {
        client,
        device,
        device_flag: selector.is_some(),
        yes: matches.get_flag("yes"),
    };
    let (name, args) = matches
        .subcommand()
        .ok_or_else(|| CliError::usage("Choose a command."))?;
    for module in korri::MODULES {
        if module.command().get_name() == name {
            return module.run(args, &context)?.print(json);
        }
    }
    Err(CliError::usage("Unknown command."))
}
