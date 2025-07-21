pub const DB_FOLDERS_NAME: &str = "folders";
pub const DB_FOLDERS_COLS: &str = "fld_id int, rjcode text primary key, path text, last_scan timestamp, active bool";

pub const DB_DLSITE_SCAN_NAME: &str = "dlsite_scan";
pub const DB_DLSITE_SCAN_COLS: &str = "fld_id int, last_scan timestamp";

pub const DB_DLSITE_TAG_NAME: &str = "dlsite_tag";
pub const DB_DLSITE_TAG_COLS: &str = "tag_id int, tag_name text primary key";

pub const DB_CIRCLE_NAME: &str = "circles";
pub const DB_CIRCLE_COLS: &str = "cir_id int, rgcode text primary key, name_en text, name_jp text";

pub const DB_LKP_WORK_CIRCLE_NAME: &str = "lkp_work_circle";
pub const DB_LKP_WORK_CIRCLE_COLS: &str = "fld_id int, cir_id int";

pub const DB_LKP_WORK_TAG_NAME: &str = "lkp_work_tag";
pub const DB_LKP_WORK_TAG_COLS: &str = "fld_id int, tag_id int";

pub const DB_RELEASE_DATE_NAME: &str = "release_date";
pub const DB_RELEASE_DATE_COLS: &str = "fld_id int, release_date datetime";

pub const DB_RATING_NAME: &str = "rating";
pub const DB_RATING_COLS: &str = "fld_id int, rating text";

pub const DB_STARS_NAME: &str = "stars";
pub const DB_STARS_COLS: &str = "fld_id int, stars float";

pub const DB_WORKS_NAME: &str = "works";
pub const DB_WORKS_COLS: &str = "fld_id int, name text, img_link text";

pub const DB_CVS_NAME: &str = "cvs";
pub const DB_CVS_COLS: &str = "cv_id int, name_jp text primary key, name_en text";

pub const DB_LKP_WORK_CVS_NAME: &str = "lkp_work_cvs";
pub const DB_LKP_WORK_CVS_COLS: &str = "fld_id int, cv_id int";

pub const DB_DLSITE_ERRORS_NAME: &str = "dlsite_errors";
pub const DB_DLSITE_ERRORS_COLS: &str = "fld_id int, error_type text";

pub const DB_DLSITE_COVERS_LINK_NAME: &str = "dlsite_covers";
pub const DB_DLSITE_COVERS_LINK_COLS: &str = "fld_id int, link text";
