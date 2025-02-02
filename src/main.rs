use std::io;

use clap::Parser;
use dlsite::{scrapper::DlSiteProductScrapResult, types::DlSiteProductIdResult};
use folders::types::ManagedFolder;
use tagger::types::WorkDetails;
mod tagger;
mod dlsite;
mod folders;

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
    let f = ManagedFolder::new("./RJ01306319");
    let hm = WorkDetails::build_from_rjcode(f.rjcode.clone()).await.unwrap();
    let hp = DlSiteProductScrapResult::build_from_rjcode(f.rjcode.clone()).await;
    println!("{f:?}");
    println!("{hm:?}");
    println!("{hp:?}");
    Ok(())
}
