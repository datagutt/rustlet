use anyhow::Result;

use crate::api::Client;
use crate::config;

pub struct Args<'a> {
    pub device_id: &'a str,
    pub installation_id: &'a str,
    pub url: Option<&'a str>,
    pub token: Option<&'a str>,
}

pub fn run(args: Args<'_>) -> Result<()> {
    let (url, token) = config::resolve_credentials(args.url, args.token)?;
    let client = Client::new(&url, &token)?;
    client.delete(args.device_id, args.installation_id)
}
