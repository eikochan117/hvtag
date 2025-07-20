use crate::folders::types::{ManagedFolder, RJCode};
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

pub fn get_max_fld_id() -> String {
    format!(
        "select 
            max(fld_id) 
        from 
            {DB_FOLDERS_NAME}")
}

pub fn insert_managed_folder(mf: &ManagedFolder, fld_id: i32) -> String {
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

pub fn remove_previous_tags_of_work(work: RJCode) -> String {
    format!(
        "with
        cte as (
            select 
                fld_id 
            from 
                {DB_FOLDERS_NAME}
            where
                rjcode = '{work}'
        )
        delete from {DB_LKP_WORK_TAG_NAME}
        where 
            fld_id in
                (select fld_id from cte)")
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
            {DB_FOLDERS_NAME} t1
        left join
            {DB_DLSITE_TAG_NAME} t2
        on 1=1
        where 
            t1.rjcode = '{work}'
            and t2.tag_name in 
            (select val from cte)")
}
