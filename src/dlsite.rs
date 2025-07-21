use rusqlite::Connection;

use crate::{database::{sql::{assign_cvs_to_work, assign_rating_to_work, assign_release_date_to_work, assign_tags_to_work, get_max_id, insert_circle, insert_cv, insert_tag, remove_previous_data_of_work, set_work_scan_date}, tables::*}, dlsite::scrapper::DlSiteProductScrapResult, errors::HvtError, folders::types::RJCode, tagger::types::WorkDetails};

pub mod api;
pub mod scrapper;
pub mod types;

pub async fn  assign_data_to_work(conn: &Connection, work: RJCode) -> Result<(), HvtError> {
    let wd = WorkDetails::build_from_rjcode(work.clone()).await
        .map_err(|x| HvtError::GenericError(x))?;
    let sr = DlSiteProductScrapResult::build_from_rjcode(work.clone()).await;

    if sr.genre.is_empty() {
        return Err(HvtError::RemovedWork(work));
    }
    
    // TAGS
    println!("assign tags: {:?}", &sr.genre);
    let mut max_tag_id: usize = conn.query_one(&get_max_id("tag_id", DB_DLSITE_TAG_NAME), [], |x| x.get(0))?;
    // register new tags
    for tag in &sr.genre {
        max_tag_id += conn.execute(&insert_tag(&tag, max_tag_id + 1), [])?;
    }

    // remove existing tags if exists
    conn.execute(&remove_previous_data_of_work(DB_LKP_WORK_TAG_NAME, work.clone()), [])?;
    
    // assign new tags
    conn.execute(&assign_tags_to_work(work.clone(), &sr.genre), [])?;

    // RELEASE DATE
    println!("assign date: {:?}", &wd.release_date);
    conn.execute(&remove_previous_data_of_work(DB_RELEASE_DATE_NAME, work.clone()), [])?;
    conn.execute(&assign_release_date_to_work(work.clone(), &wd.release_date), [])?;

    // CIRCLE
    println!("assign circle: {:?}", &wd.maker_code);
    conn.execute(&remove_previous_data_of_work(DB_LKP_WORK_CIRCLE_NAME, work.clone()), [])?;
    let max_cir_id: usize = conn.query_one(&get_max_id("cir_id", DB_CIRCLE_NAME), [], |x| x.get(0))?;
    conn.execute(&insert_circle(wd.maker_code, "", "", max_cir_id + 1), [])?;

    // RATING
    println!("assign rating: {}", &wd.age_category);
    conn.execute(&remove_previous_data_of_work(DB_RATING_NAME, work.clone()), [])?;
    conn.execute(&assign_rating_to_work(work.clone(), &wd.age_category.to_string()), [])?;
    
    // CVS
    println!("assign cvs: {:?}", &sr.cvs);
    let mut max_cv_id: usize = conn.query_one(&get_max_id("cv_id", DB_CVS_NAME), [], |x| x.get(0))?;
    for cv in &sr.cvs {
        max_cv_id += conn.execute(&insert_cv(&cv, "", max_cv_id + 1), [])?;
    }

    conn.execute(&remove_previous_data_of_work(DB_LKP_WORK_CVS_NAME, work.clone()), [])?;
    conn.execute(&assign_cvs_to_work(work.clone(), &sr.cvs), [])?;



    conn.execute(&set_work_scan_date(work.clone()), [])?;
    Ok(())
}
