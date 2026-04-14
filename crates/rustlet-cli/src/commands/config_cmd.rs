use anyhow::Result;
use clap::Subcommand;

use crate::config;

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set a config value (valid keys: url, token).
    Set {
        /// Config key: `url` or `token`.
        key: String,
        /// Value to store.
        value: String,
    },
    /// Print a config value to stdout (valid keys: url, token).
    Get {
        /// Config key: `url` or `token`.
        key: String,
    },
    /// Print the path to the config file.
    Path,
}

pub fn run(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Set { key, value } => {
            config::set_value(&key, &value)?;
            let path = config::config_path()?;
            eprintln!("wrote {} to {}", key, path.display());
        }
        ConfigAction::Get { key } => match config::get_value(&key)? {
            Some(v) => println!("{v}"),
            None => std::process::exit(1),
        },
        ConfigAction::Path => {
            println!("{}", config::config_path()?.display());
        }
    }
    Ok(())
}
