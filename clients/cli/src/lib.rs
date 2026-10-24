pub mod acquisition;
pub mod client;
pub mod controllers;
pub mod devices;
pub mod error;
pub mod identity;
pub mod launch_settings;
pub mod library;
pub mod metadata;
pub mod module;
pub mod output;
pub mod play;
pub mod plugins;
mod private_terminal;
pub mod prompt;
pub mod saves;
mod settings_input;
pub mod setup;
pub mod status;
pub mod target;
pub mod wifi;

use clap::{Arg, ArgAction, Command};
use module::Module;
use std::sync::LazyLock;

// Areas add their module here. No transport, output, or target-policy edits.
pub static MODULES: &[&dyn Module] = &[
    &status::Status,
    &devices::Devices,
    &library::Library,
    &play::Play,
    &play::Leave,
    &play::Resume,
    &play::End,
    &launch_settings::LaunchSettings,
    &acquisition::Get,
    &acquisition::Downloads,
    &controllers::Controllers,
    &plugins::Plugins,
    &wifi::Wifi,
    &saves::Saves,
    &identity::Identity,
    &setup::Setup,
    &metadata::Metadata,
];

pub static VERSION: LazyLock<String> = LazyLock::new(|| match option_env!("KORRI_BUILD_ID") {
    Some(identity) => format!("{} ({identity})", env!("CARGO_PKG_VERSION")),
    None => env!("CARGO_PKG_VERSION").into(),
});

pub fn command() -> Command {
    let mut command = Command::new("korri")
        .about("Control Korri on this device")
        .color(clap::ColorChoice::Never)
        .version(VERSION.clone())
        .arg_required_else_help(true)
        .arg(
            Arg::new("json")
                .long("json")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Print unchanged RPC responses as JSON"),
        )
        .arg(
            Arg::new("yes")
                .long("yes")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Confirm destructive actions; never grants plugin permissions"),
        )
        .arg(
            Arg::new("device")
                .long("device")
                .global(true)
                .value_name("NAME_OR_KEY")
                .help("Device performing the operation, not a streaming route"),
        )
        .arg(
            Arg::new("socket")
                .long("socket")
                .global(true)
                .value_name("PATH")
                .help("Local korrid person socket"),
        )
        .subcommand(
            Command::new("version")
                .about("Show the installed CLI version without contacting korrid"),
        )
        .subcommand(
            Command::new("completion")
                .about("Generate shell completion without contacting korrid")
                .arg(
                    Arg::new("shell")
                        .required(true)
                        .value_parser(clap::value_parser!(clap_complete::Shell)),
                ),
        );
    for module in MODULES {
        command = command.subcommand(module.command());
    }
    command
}
