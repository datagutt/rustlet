use anyhow::Result;
use rustlet_encode::Filter;

pub fn run() -> Result<()> {
    let longest = Filter::ALL.iter().map(|f| f.name().len()).max().unwrap_or(0);
    for filter in Filter::ALL {
        println!("{:<width$}  {}", filter.name(), filter.describe(), width = longest);
    }
    Ok(())
}
