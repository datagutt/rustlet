use std::fs;
use std::io::{self, Read};
use std::path::Path;

use anyhow::{Context, Result};

use crate::api::Client;
use crate::config;

pub struct Args<'a> {
    pub device_id: &'a str,
    pub image: &'a Path,
    pub installation_id: Option<&'a str>,
    pub background: bool,
    pub url: Option<&'a str>,
    pub token: Option<&'a str>,
}

pub fn run(args: Args<'_>) -> Result<()> {
    let bytes = read_image(args.image)?;
    let (url, token) = config::resolve_credentials(args.url, args.token)?;
    let client = Client::new(&url, &token)?;
    client.push(
        args.device_id,
        &bytes,
        args.installation_id,
        args.background,
    )
}

fn read_image(path: &Path) -> Result<Vec<u8>> {
    if path.as_os_str() == "-" {
        let mut buf = Vec::new();
        io::stdin()
            .read_to_end(&mut buf)
            .context("reading image from stdin")?;
        return Ok(buf);
    }
    fs::read(path).with_context(|| format!("reading {}", path.display()))
}
