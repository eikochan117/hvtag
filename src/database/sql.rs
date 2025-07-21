use crate::folders::types::{ManagedFolder, RGCode, RJCode};
use crate::database::tables::*;

pub fn init_db() -> String {
    format!(
        "create table if not exists db_init as 
            select 
                datetime() as init_dte")
}

pub fn init_table(name: &str, cols: &str) -> String {
    format!(
        "create table if not exists {name} ({cols})")
}

pub fn get_max_id(id_fld: &str, table: &str) -> String {
    format!(
        "select 
            coalesce(max({id_fld}), 1) 
        from 
            {table}")
}

pub fn insert_managed_folder(mf: &ManagedFolder, fld_id: usize) -> String {
    let path = &mf.path;
    let rjcode = &mf.rjcode;
    format!(
        "insert or ignore into {DB_FOLDERS_NAME} values
            (
                {fld_id},
                '{rjcode}',
                '{path}',
                datetime(),
                true
            )")
}

pub fn get_unscanned_works() -> String {
    format!(
        "select 
            t1.rjcode 
        from 
            {DB_FOLDERS_NAME} t1
        left join
            {DB_DLSITE_SCAN_NAME} t2
        using(fld_id)
        where
            t2.last_scan is null")
}

pub fn insert_tag(tag: &str, tag_id: usize) -> String {
    format!(
        "insert or ignore into {DB_DLSITE_TAG_NAME} values
            (
                {tag_id},
                '{tag}'
            )")
}

pub fn insert_circle(circle: RGCode, en_name: &str, jp_name: &str, cir_id: usize) -> String {
    format!(
        "insert or replace into {DB_CIRCLE_NAME} values
            (
                {cir_id},
                '{circle}',
                '{en_name}',
                '{jp_name}'
            )")
}

pub fn insert_cv(jp_name: &str, en_name: &str, cv_id: usize) -> String {
    format!(
        "insert or replace into {DB_CVS_NAME} values
            (
                {cv_id},
                '{jp_name}',
                '{en_name}'
            )")
}

pub fn remove_previous_data_of_work(table: &str, work: RJCode) -> String {
    let sql = format!(
        "with
        cte as (
            select 
                fld_id 
            from 
                {DB_FOLDERS_NAME}
            where
                rjcode = '{work}'
        )
        delete from {table}
        where 
            fld_id in
                (select fld_id from cte)");
    println!("{sql}");
    sql
}

pub fn assign_release_date_to_work(work: RJCode, date: &str) -> String {
    format!(
        "insert into {DB_RELEASE_DATE_NAME}
        select
            t1.fld_id,
            cast('{date}' as datetime) as release_date
        from
            {DB_FOLDERS_NAME} t1
        where
            t1.rjcode = '{work}'")
}

pub fn assign_circle_to_work(work: RJCode, circle: RGCode) -> String {
    format!(
        "insert into {DB_LKP_WORK_CIRCLE_NAME}
        select
            t1.fld_id,
            t2.cir_id
        from
            {DB_FOLDERS_NAME} t1,
            {DB_CIRCLE_NAME} t2
        where
            t1.rjcode = '{work}'
            and t2.rgcode = '{circle}'")

}

pub fn assign_tags_to_work(work: RJCode, tags: &Vec<String>) -> String {
    let cte_parts : Vec<String> = tags.iter()
        .map(|x| format!("select '{x}' as val"))
        .collect();
    let joint_part = cte_parts.join(" union all ");
    format!(
        "with cte as ({joint_part})
        insert into {DB_LKP_WORK_TAG_NAME}
        select 
            t1.fld_id,
            t2.tag_id
        from
            {DB_FOLDERS_NAME} t1,
            {DB_DLSITE_TAG_NAME} t2
        where 
            t1.rjcode = '{work}'
            and t2.tag_name in 
            (select val from cte)")
}

pub fn assign_rating_to_work(work: RJCode, rating: &str) -> String {
    format!(
        "insert into {DB_RATING_NAME}
        select
            t1.fld_id,
            '{rating}' as rating
        from
            {DB_FOLDERS_NAME} t1
        where
            t1.rjcode = '{work}'")
}

pub fn assign_stars_to_work(work: RJCode, stars: f32) -> String {
    format!(
        "insert into {DB_STARS_COLS}
        select
            t1.fld_id,
            '{stars}' as stars
        from
            {DB_FOLDERS_NAME} t1
        where
            t1.rjcode = '{work}'")
}

pub fn assign_cvs_to_work(work: RJCode, cvs: &Vec<String>) -> String {
    let cte_parts : Vec<String> = cvs.iter()
        .map(|x| format!("select '{x}' as val"))
        .collect();
    let joint_part = cte_parts.join(" union all ");
    format!(
        "with cte as ({joint_part})
        insert into {DB_LKP_WORK_CVS_NAME}
        select 
            t1.fld_id,
            t2.cv_id
        from
            {DB_FOLDERS_NAME} t1,
            {DB_CVS_NAME} t2
        where 
            t1.rjcode = '{work}'
            and t2.name_jp in 
            (select val from cte)")
}

pub fn set_work_scan_date(work: RJCode) -> String {
    format!(
    "insert or replace into {DB_DLSITE_SCAN_NAME}
    select 
        t1.fld_id,
        datetime() as last_scan
    from {DB_FOLDERS_NAME} t1
        where rjcode = '{work}'")
}
