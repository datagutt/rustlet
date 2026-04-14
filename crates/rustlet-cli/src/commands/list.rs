use anyhow::Result;

use crate::api::Client;
use crate::commands::devices::OutputFormat;
use crate::config;

pub struct Args<'a> {
    pub device_id: &'a str,
    pub url: Option<&'a str>,
    pub token: Option<&'a str>,
    pub format: OutputFormat,
}

pub fn run(args: Args<'_>) -> Result<()> {
    let (url, token) = config::resolve_credentials(args.url, args.token)?;
    let client = Client::new(&url, &token)?;
    let installations = client.installations(args.device_id)?;
    if installations.is_empty() {
        if matches!(args.format, OutputFormat::Tsv) {
            eprintln!("no installations found on device {}", args.device_id);
        }
        return Ok(());
    }
    for i in installations {
        match args.format {
            OutputFormat::Tsv => println!("{}\t{}", i.id, i.app_id),
            OutputFormat::IdsOnly => println!("{}", i.id),
        }
    }
    Ok(())
}
