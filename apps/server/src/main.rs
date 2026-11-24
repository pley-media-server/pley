// clippy.toml's rules hold for the program (src/lib.rs).
#![cfg_attr(not(test), warn(clippy::disallowed_methods))]

use std::process::ExitCode;

use clap::Parser;
use mimalloc::MiMalloc;
use pley::config::{Cli, Command, Config};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    let command = match &cli.command {
        None | Some(Command::Serve) => return Ok(serve(&cli)),
        Some(Command::SaveConfig { discovery }) => {
            let path = cli.config.clone().unwrap_or_else(|| cli.data_dir_or_default().join("pley.toml"));
            pley::config::save_config(&cli, &path, *discovery)?;
            println!("saved in {}", path.display());
            return Ok(ExitCode::SUCCESS);
        }
        #[cfg(windows)]
        Some(Command::Service(command)) => return pley::service::main(&cli, command),
        Some(command) => command,
    };
    // Commands run from a terminal: an error is printed there by `main`.
    let config = Config::load(&cli)?;
    pley::logging::init(&cli.log, None);
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async move {
        match command {
            Command::ResetPassword { username, password, admin } => {
                let password = match password {
                    Some(password) => password.clone(),
                    None => pley::read_password(username)?,
                };
                pley::reset_password(config, username, &password, *admin).await
            }
            Command::Health => pley::health(&config).await,
            Command::Serve | Command::SaveConfig { .. } => unreachable!(),
            #[cfg(windows)]
            Command::Service(_) => unreachable!(),
        }
    })?;
    Ok(ExitCode::SUCCESS)
}

/// `pley serve`. Started at login or from Explorer, its console closes as
/// soon as it exits (and the tray detaches from it anyway): why it could not
/// start must be in the log file too.
fn serve(cli: &Cli) -> ExitCode {
    let config = match Config::load(cli) {
        Ok(config) => config,
        Err(err) => {
            pley::logging::init(&cli.log, Some(&Config::fallback_log_file(cli)));
            tracing::error!("Pley could not start: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    pley::logging::init(&cli.log, Some(&config.log_file()));
    match pley::serve(config, !cli.no_tray) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("Pley could not start: {err:#}");
            ExitCode::FAILURE
        }
    }
}
