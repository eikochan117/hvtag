use std::io;

use clap::Parser;
use database::db_loader::get_default_db_path;
use dlsite::{scrapper::DlSiteProductScrapResult, types::DlSiteProductIdResult};
use folders::types::ManagedFolder;
use tagger::types::WorkDetails;

use crate::database::{db_loader::open_db, init};

mod errors;
use errors::*;
mod tagger;
mod dlsite;
mod folders;
mod database;

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
    let db = open_db(None)?;
    init(&db)?;
    // let mut s = String::new();
    // io::stdin().read_line(&mut s).unwrap();
    // let sa = format!("prgm {s}");
    // let sa = sa.split(" ");
    // let a = PrgmArgs::try_parse_from(sa)?;
    // println!("{a:?}");
    //let f = ManagedFolder::new("./RJ01306319");
    //let hm = WorkDetails::build_from_rjcode(f.rjcode.clone()).await.unwrap();
    //let hp = DlSiteProductScrapResult::build_from_rjcode(f.rjcode.clone()).await;
    //println!("{f:?}");
    //println!("{hm:?}");
    //println!("{hp:?}");
    let p = get_default_db_path();
    println!("{p:?}");
    Ok(())
}
