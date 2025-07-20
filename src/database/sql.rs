use crate::folders::types::ManagedFolder;
use crate::database::tables::*;

pub fn init_db() -> String {
    format!(
        "create table if not exists db_init as select datetime() as init_dte")
}

pub fn init_table(name: &str, cols: &str) -> String {
    format!(
        "create table if not exists {name} ({cols})")
}

pub fn get_max_fld_id() -> String {
    format!(
        "select max(fld_id) from {DB_FOLDERS_NAME}")
}

pub fn insert_managed_folder(mf: &ManagedFolder, fld_id: i32) -> String {
    let path = &mf.path;
    let rjcode = &mf.rjcode;
    format!(
        "insert or ignore into {DB_FOLDERS_NAME} values
        ({fld_id}, '{rjcode}', '{path}', datetime(), true)")
}
