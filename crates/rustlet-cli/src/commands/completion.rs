//! `rustlet completion <shell>` — prints a shell completion script to
//! stdout. Uses clap_complete's built-in generators; each shell's install
//! path differs so we leave those instructions in the subcommand's help.

use std::io;

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::Cli;

pub fn run(shell: Shell) -> Result<()> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut io::stdout());
    Ok(())
}
