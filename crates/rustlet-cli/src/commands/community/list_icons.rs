use anyhow::Result;

pub fn run() -> Result<()> {
    println!("rustlet does not ship an icon registry.");
    println!();
    println!("Schema fields accept any icon identifier the downstream display device");
    println!("recognises. Tronbyt firmware resolves icon names against its own");
    println!("bundled FontAwesome set. Consult the Tronbyt icon reference when choosing");
    println!("names for `schema.Text`, `schema.Dropdown`, and similar fields.");
    println!();
    println!("`rustlet community validate-icons` performs a soft syntactic check on");
    println!("each schema field's icon (non-empty ASCII identifier).");
    Ok(())
}
