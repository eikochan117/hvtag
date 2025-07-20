
use clap::Parser;
use database::db_loader::get_default_db_path;
use dlsite::{scrapper::DlSiteProductScrapResult, types::DlSiteProductIdResult};

use crate::{database::{db_loader::open_db, init, sql::{assign_tags_to_work, get_max_fld_id, insert_managed_folder, insert_tag, remove_previous_tags_of_work}}, folders::{get_list_of_folders, get_list_of_unscanned_works}, tagger::types::WorkDetails};

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
    let x = get_list_of_unscanned_works(&db, Some(5))?;
    let mut tag_id = 1;
    for i in x {
        let hm = WorkDetails::build_from_rjcode(i.clone()).await.unwrap();
        let hp = DlSiteProductScrapResult::build_from_rjcode(i.clone()).await;
        println!("{hm:?}");
        println!("{hp:?}");
        for tag in &hp.genre {
            tag_id += db.execute(&insert_tag(&tag, tag_id), [])?;
        }
        db.execute(&remove_previous_tags_of_work(i.clone()), [])?;
        db.execute(&assign_tags_to_work(i, &hp.genre), [])?;
    }
    //println!("{f:?}");
    //let p = get_default_db_path();
    //println!("{p:?}");
    //...
    //println!("{p:?}");
    //let mut max_id: i32 = db.query_one(&get_max_fld_id(), [], |x| {
    //    x.get(0)
    //}).unwrap_or_default();
    //for f in p {
    //    db.execute(
    //    //println!("{}", 
    //    &insert_managed_folder(&f, max_id + 1)
    //    //);
    //    , [])?;
    //    max_id += 1
    //}
    Ok(())
}
