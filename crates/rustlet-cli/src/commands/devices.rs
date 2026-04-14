use anyhow::Result;

use crate::api::Client;
use crate::config;

pub struct Args<'a> {
    pub url: Option<&'a str>,
    pub token: Option<&'a str>,
}

pub fn run(args: Args<'_>) -> Result<()> {
    let (url, token) = config::resolve_credentials(args.url, args.token)?;
    let client = Client::new(&url, &token)?;
    let devices = client.devices()?;
    if devices.is_empty() {
        eprintln!("no devices found");
        return Ok(());
    }
    for d in devices {
        println!("{}\t{}", d.id, d.display_name);
    }
    Ok(())
}
