use std::{env, fs, path::Path};

pub fn get_default_db_path() -> Option<String> {
    let os = std::env::consts::OS;
    match os {
        "windows" => {
            let username = env::var("USERNAME").expect("Could not get %USERNAME% value");
            let path_f = format!("C:\\Users\\{username}\\AppData\\Local\\hvtag");
            let path = Path::new(&path_f);
            if !path.exists() {
                fs::create_dir_all(path).expect(&format!("Could not create path {path_f}"));
            }

            path.to_str().map(|x| format!("{x}\\data.ddb"))
        },
        x => {panic!( "Operating System is not supported ({x})") }
    }
}

pub fn open_db(custom_path: Option<&str>) -> Result<DuckDB, _> {

}
