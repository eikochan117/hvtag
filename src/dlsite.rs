use rusqlite::Connection;
use tracing::{debug, warn};

use crate::{database::{queries, tables::*}, dlsite::scrapper::DlSiteProductScrapResult, errors::HvtError, folders::types::RJCode, tagger::types::WorkDetails};

pub mod api;
pub mod scrapper;
pub mod types;

#[derive(Default, Clone)]
pub struct DataSelection {
    pub tags: bool,
    pub release_date: bool,
    pub circle: bool,
    pub rating: bool,
    pub cvs: bool,
    pub stars: bool,
    pub cover_link: bool
}

pub async fn assign_data_to_work(
    conn: &Connection,
    work: RJCode,
    data_selection: DataSelection,
) -> Result<(), HvtError> {
    assign_data_to_work_with_client(conn, work, data_selection, None).await
}

pub async fn assign_data_to_work_with_client(
    conn: &Connection,
    work: RJCode,
    data_selection: DataSelection,
    client: Option<&reqwest::Client>,
) -> Result<(), HvtError> {
    let wd = WorkDetails::build_from_rjcode_with_client(work.as_str().to_string(), client).await
        .map_err(|x: Box<dyn std::error::Error>| HvtError::Http(x.to_string()))?;
    let sr = DlSiteProductScrapResult::build_from_rjcode_with_client(work.as_str().to_string(), client).await;

    if sr.genre.is_empty() {
        return Err(HvtError::RemovedWork(work));
    }

    // Insert work name (always do this regardless of data_selection)
    queries::insert_work_name(conn, &work, &wd.name)?;

    // TAGS
    if data_selection.tags {
        debug!("assign tags: {:?}", &sr.genre);

        // Convert all tags to lowercase
        let tags_lowercase: Vec<String> = sr.genre.iter()
            .map(|tag| tag.to_lowercase())
            .collect();

        let mut max_tag_id = queries::get_max_id(conn, "tag_id", DB_DLSITE_TAG_NAME)?;

        // register new tags (lowercase)
        for tag in &tags_lowercase {
            max_tag_id += queries::insert_tag(conn, tag, max_tag_id + 1)?;
        }

        // remove existing tags if exists and assign new tags
        queries::remove_previous_data_of_work(conn, DB_LKP_WORK_TAG_NAME, &work)?;
        queries::assign_tags_to_work(conn, &work, &tags_lowercase)?;
    }

    // RELEASE DATE
    if data_selection.release_date {
        debug!("assign date: {:?}", &wd.release_date);
        queries::remove_previous_data_of_work(conn, DB_RELEASE_DATE_NAME, &work)?;
        queries::assign_release_date_to_work(conn, &work, &wd.release_date)?;
    }

    // CIRCLE
    if data_selection.circle {
        debug!("assign circle: {:?}", &wd.maker_code);

        // Check if circle already exists in database
        let circle_exists = queries::circle_exists(conn, &wd.maker_code)?;

        if !circle_exists {
            debug!("Circle {} not in database, scraping names...", &wd.maker_code);
            let max_cir_id = queries::get_max_id(conn, "cir_id", DB_CIRCLE_NAME)?;

            // Scrape circle names from circle profile page title
            let (circle_name_en, circle_name_jp) = match scrapper::scrape_circle_profile(
                wd.maker_code.as_str(),
                client,
            ).await {
                Ok((en, jp)) => (en, jp),
                Err(e) => {
                    warn!("Failed to scrape circle profile for {}: {}. Using fallback.", wd.maker_code, e);
                    (String::new(), String::new())
                }
            };

            // Insert circle with BOTH names (EN, JP)
            queries::insert_circle(conn, &wd.maker_code, &circle_name_en, &circle_name_jp, max_cir_id + 1)?;
        } else {
            debug!("Circle {} already in database, skipping scrape", &wd.maker_code);
        }

        // Remove previous assignment before creating new one
        queries::remove_previous_data_of_work(conn, DB_LKP_WORK_CIRCLE_NAME, &work)?;

        // Assign circle to work
        queries::assign_circle_to_work(conn, &work, &wd.maker_code)?;
    }

    // RATING
    if data_selection.rating {
        debug!("assign rating: {}", &wd.age_category);
        queries::remove_previous_data_of_work(conn, DB_RATING_NAME, &work)?;
        queries::assign_rating_to_work(conn, &work, &wd.age_category.to_string())?;
    }

    // CVS
    if data_selection.cvs {
        debug!("assign cvs: {:?}", &sr.cvs);
        let mut max_cv_id = queries::get_max_id(conn, "cv_id", DB_CVS_NAME)?;

        for cv in &sr.cvs {
            max_cv_id += queries::insert_cv(conn, cv, "", max_cv_id + 1)?;
        }

        queries::remove_previous_data_of_work(conn, DB_LKP_WORK_CVS_NAME, &work)?;
        queries::assign_cvs_to_work(conn, &work, &sr.cvs)?;
    }

    // COVER LINK
    if data_selection.cover_link {
        queries::remove_previous_data_of_work(conn, DB_DLSITE_COVERS_LINK_NAME, &work)?;
        queries::assign_cover_link_to_work(conn, &work, &wd.image_link)?;
    }

    // STARS
    if data_selection.stars {
        queries::remove_previous_data_of_work(conn, DB_STARS_NAME, &work)?;
        queries::assign_stars_to_work(conn, &work, wd.rate)?;
    }

    queries::set_work_scan_date(conn, &work)?;
    Ok(())
}
