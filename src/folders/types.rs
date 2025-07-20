use std::{fs::{read_dir, DirEntry}, path::Path};

#[derive(Debug)]
pub struct ManagedFile {
    filename: String,
    extension: String,
    path: String,

}

impl ManagedFile {
    pub fn from_direntry(e: DirEntry) -> Self {
        let extension = e.file_name().into_string().unwrap().split(".").last().unwrap().to_string();
        ManagedFile {
            filename: e.file_name().into_string().unwrap(),
            extension,
            path: e.path().display().to_string()
        }
    }
}

#[derive(Debug)]
pub struct ManagedFolder {
    pub is_valid: bool,
    pub is_tagged: bool,
    //pub has_other_filetypes: bool,
    pub has_cover: bool,
    //pub database_id: Option<String>,
    pub rjcode: String,
    pub path: String,
    pub files: Vec<ManagedFile>,
}

impl ManagedFolder {
    pub fn new(path: String) -> Self {
        let p = Path::new(&path);
        let mut files = vec![];
        match read_dir(p) {
            Ok(entries) => {
                for e in entries {
                    if let Ok(en) = e {
                        if Path::new(&en.path()).is_file() {
                            files.push(ManagedFile::from_direntry(en));
                        }
                    }
                }
            }

            Err(x) => {
                panic!("Could not read dir {} : {x}", p.to_str().unwrap());
            }
        };

        let is_tagged = files.iter().any(|x| x.extension == "tagged");
        let has_cover = files.iter().any(|x| x.filename == "folder.jpeg");
        let rjcode = p.file_name().unwrap().to_str().unwrap().to_string();

        let is_valid = 
            !files.is_empty()
            && rjcode.starts_with("RJ");
        ManagedFolder {
            is_valid,
            path: path.to_string(),
            files,
            is_tagged,
            has_cover,
            rjcode

        }
    }
}
