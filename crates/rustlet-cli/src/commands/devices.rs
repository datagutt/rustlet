use anyhow::Result;

use crate::api::Client;
use crate::config;

pub struct Args<'a> {
    pub url: Option<&'a str>,
    pub token: Option<&'a str>,
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    /// Tab-separated `id\tdisplay_name` per line.
    Tsv,
    /// One device id per line. Used by shell completion.
    IdsOnly,
}

pub fn run(args: Args<'_>) -> Result<()> {
    let (url, token) = config::resolve_credentials(args.url, args.token)?;
    let client = Client::new(&url, &token)?;
    let devices = client.devices()?;
    if devices.is_empty() {
        if matches!(args.format, OutputFormat::Tsv) {
            eprintln!("no devices found");
        }
        return Ok(());
    }
    for d in devices {
        match args.format {
            OutputFormat::Tsv => println!("{}\t{}", d.id, d.display_name),
            OutputFormat::IdsOnly => println!("{}", d.id),
        }
    }
    Ok(())
}
