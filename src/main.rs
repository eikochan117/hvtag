
use clap::Parser;
use database::db_loader::get_default_db_path;
use dlsite::{scrapper::DlSiteProductScrapResult, types::DlSiteProductIdResult};

use crate::{database::{db_loader::open_db, init, sql::{assign_tags_to_work, insert_managed_folder, insert_tag}}, dlsite::assign_data_to_work, folders::{get_list_of_folders, get_list_of_unscanned_works}, tagger::types::WorkDetails};

mod errors;
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
    //let x = get_list_of_unscanned_works(&db, Some(5))?;
    let x = get_list_of_unscanned_works(&db, None)?;
    for i in x {
        println!("{i}");
        assign_data_to_work(&db, i).await?;
    //    for tag in &hp.genre {
    //        tag_id += db.execute(&insert_tag(&tag, tag_id), [])?;
    //    }
    //    db.execute(&remove_previous_tags_of_work(i.clone()), [])?;
    //    db.execute(&assign_tags_to_work(i, &hp.genre), [])?;
    }
    //println!("{f:?}");
    //let p = get_default_db_path();
    //println!("{p:?}");
    //...
    //println!("{p:?}");
    Ok(())
}
