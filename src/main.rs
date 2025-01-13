use std::io;

use clap::Parser;
use types::{dlsite::DlSiteProductScrapResult, local::WorkDetails};
mod tagger;
mod types;
mod dlsite;

#[derive(Parser, Debug)]
struct PrgmArgs {

    /// Download images directly from Dlsite
    #[arg(long)]
    image: bool,

    /// Convert files to .mp3 320kbps
    #[arg(long)]
    convert: bool,

    /// Move tagged files to destination
    #[arg(long)]
    r#move: Option<String>,

    /// Directory to tag
    #[arg(long)]
    input: Option<String>,

    /// RJCode of the current directory
    #[arg(long)]
    rjcode: Option<String>
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let args = PrgmArgs::parse();
    // let mut s = String::new();
    // io::stdin().read_line(&mut s).unwrap();
    // let sa = format!("prgm {s}");
    // let sa = sa.split(" ");
    // let a = PrgmArgs::try_parse_from(sa)?;
    // println!("{a:?}");
    let hm = WorkDetails::build_from_rjcode("RJ01293993".to_string()).await.unwrap();
    let hp = DlSiteProductScrapResult::build_from_rjcode("RJ01293993".to_string()).await;
    Ok(())
}
