use anyhow::Result;

use crate::api::Client;
use crate::config;

pub struct Args<'a> {
    pub device_id: &'a str,
    pub url: Option<&'a str>,
    pub token: Option<&'a str>,
}

pub fn run(args: Args<'_>) -> Result<()> {
    let (url, token) = config::resolve_credentials(args.url, args.token)?;
    let client = Client::new(&url, &token)?;
    let installations = client.installations(args.device_id)?;
    if installations.is_empty() {
        eprintln!("no installations found on device {}", args.device_id);
        return Ok(());
    }
    for i in installations {
        println!("{}\t{}", i.id, i.app_id);
    }
    Ok(())
}
