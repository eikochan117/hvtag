pub const DB_FOLDERS_NAME: &str = "folders";
pub const DB_FOLDERS_COLS: &str = "fld_id INTEGER PRIMARY KEY, rjcode TEXT NOT NULL UNIQUE, path TEXT, last_scan TEXT, active BOOLEAN";

pub const DB_DLSITE_SCAN_NAME: &str = "dlsite_scan";
pub const DB_DLSITE_SCAN_COLS: &str = "fld_id INTEGER NOT NULL, \
    last_scan TEXT, \
    FOREIGN KEY (fld_id) REFERENCES folders(fld_id) ON DELETE CASCADE";

pub const DB_DLSITE_TAG_NAME: &str = "dlsite_tag";
pub const DB_DLSITE_TAG_COLS: &str = "tag_id INTEGER PRIMARY KEY, tag_name TEXT NOT NULL UNIQUE";

pub const DB_CIRCLE_NAME: &str = "circles";
pub const DB_CIRCLE_COLS: &str = "cir_id INTEGER PRIMARY KEY, rgcode TEXT NOT NULL UNIQUE, name_en TEXT, name_jp TEXT";

pub const DB_LKP_WORK_CIRCLE_NAME: &str = "lkp_work_circle";
pub const DB_LKP_WORK_CIRCLE_COLS: &str = "fld_id INTEGER NOT NULL, \
    cir_id INTEGER NOT NULL, \
    PRIMARY KEY (fld_id, cir_id), \
    FOREIGN KEY (fld_id) REFERENCES folders(fld_id) ON DELETE CASCADE, \
    FOREIGN KEY (cir_id) REFERENCES circles(cir_id) ON DELETE CASCADE";

pub const DB_LKP_WORK_TAG_NAME: &str = "lkp_work_tag";
pub const DB_LKP_WORK_TAG_COLS: &str = "fld_id INTEGER NOT NULL, \
    tag_id INTEGER NOT NULL, \
    PRIMARY KEY (fld_id, tag_id), \
    FOREIGN KEY (fld_id) REFERENCES folders(fld_id) ON DELETE CASCADE, \
    FOREIGN KEY (tag_id) REFERENCES dlsite_tag(tag_id) ON DELETE CASCADE";

pub const DB_RELEASE_DATE_NAME: &str = "release_date";
pub const DB_RELEASE_DATE_COLS: &str = "fld_id INTEGER NOT NULL, \
    release_date TEXT, \
    FOREIGN KEY (fld_id) REFERENCES folders(fld_id) ON DELETE CASCADE";

pub const DB_RATING_NAME: &str = "rating";
pub const DB_RATING_COLS: &str = "fld_id INTEGER NOT NULL, \
    rating TEXT, \
    FOREIGN KEY (fld_id) REFERENCES folders(fld_id) ON DELETE CASCADE";

pub const DB_STARS_NAME: &str = "stars";
pub const DB_STARS_COLS: &str = "fld_id INTEGER NOT NULL, \
    stars REAL, \
    FOREIGN KEY (fld_id) REFERENCES folders(fld_id) ON DELETE CASCADE";

pub const DB_WORKS_NAME: &str = "works";
pub const DB_WORKS_COLS: &str = "fld_id INTEGER NOT NULL, \
    name TEXT, \
    img_link TEXT, \
    FOREIGN KEY (fld_id) REFERENCES folders(fld_id) ON DELETE CASCADE";

pub const DB_CVS_NAME: &str = "cvs";
pub const DB_CVS_COLS: &str = "cv_id INTEGER PRIMARY KEY, name_jp TEXT NOT NULL UNIQUE, name_en TEXT";

pub const DB_LKP_WORK_CVS_NAME: &str = "lkp_work_cvs";
pub const DB_LKP_WORK_CVS_COLS: &str = "fld_id INTEGER NOT NULL, \
    cv_id INTEGER NOT NULL, \
    PRIMARY KEY (fld_id, cv_id), \
    FOREIGN KEY (fld_id) REFERENCES folders(fld_id) ON DELETE CASCADE, \
    FOREIGN KEY (cv_id) REFERENCES cvs(cv_id) ON DELETE CASCADE";

pub const DB_DLSITE_ERRORS_NAME: &str = "dlsite_errors";
pub const DB_DLSITE_ERRORS_COLS: &str = "fld_id INTEGER NOT NULL, \
    error_type TEXT, \
    error_timestamp TEXT, \
    retry_count INTEGER, \
    error_category TEXT, \
    error_details TEXT, \
    is_resolved BOOLEAN, \
    resolved_date TEXT, \
    FOREIGN KEY (fld_id) REFERENCES folders(fld_id) ON DELETE CASCADE";

pub const DB_DLSITE_COVERS_LINK_NAME: &str = "dlsite_covers";
pub const DB_DLSITE_COVERS_LINK_COLS: &str = "fld_id INTEGER NOT NULL, \
    link TEXT, \
    FOREIGN KEY (fld_id) REFERENCES folders(fld_id) ON DELETE CASCADE";

// New tables for file-level tracking and history
pub const DB_FILE_PROCESSING_NAME: &str = "file_processing";
pub const DB_FILE_PROCESSING_COLS: &str = "file_id integer primary key autoincrement, \
    fld_id int not null, \
    file_path text not null unique, \
    file_name text not null, \
    file_extension text, \
    file_size_bytes integer, \
    is_tagged boolean default 0, \
    tag_date text, \
    is_converted boolean default 0, \
    convert_date text, \
    conversion_error text, \
    is_moved boolean default 0, \
    move_date text, \
    move_destination text, \
    last_processed text, \
    processing_status text default 'pending', \
    foreign key (fld_id) references folders(fld_id)";

pub const DB_PROCESSING_HISTORY_NAME: &str = "processing_history";
pub const DB_PROCESSING_HISTORY_COLS: &str = "event_id integer primary key autoincrement, \
    fld_id int not null, \
    file_path text, \
    operation_type text not null, \
    stage text not null, \
    status text not null, \
    error_message text, \
    retry_count integer default 0, \
    duration_ms integer, \
    executed_at text default current_timestamp, \
    completed_at text, \
    metadata text, \
    foreign key (fld_id) references folders(fld_id)";

pub const DB_METADATA_HISTORY_NAME: &str = "metadata_history";
pub const DB_METADATA_HISTORY_COLS: &str = "history_id integer primary key autoincrement, \
    fld_id int not null, \
    metadata_type text not null, \
    old_value text, \
    new_value text, \
    changed_at text default current_timestamp, \
    change_reason text, \
    source text, \
    foreign key (fld_id) references folders(fld_id)";

// Custom tag mappings - mapping GLOBAL des tags DLSite vers tags personnalisés
// Un seul mapping par tag DLSite, s'applique à TOUTES les œuvres
// is_ignored = 1 : le tag est ignoré lors du tagging (custom_tag_name peut être NULL)
// is_ignored = 0 : le tag est renommé en custom_tag_name
pub const DB_CUSTOM_TAG_MAPPINGS_NAME: &str = "custom_tag_mappings";
pub const DB_CUSTOM_TAG_MAPPINGS_COLS: &str = "dlsite_tag_id INTEGER PRIMARY KEY, \
    custom_tag_name TEXT, \
    is_ignored BOOLEAN DEFAULT 0, \
    created_at TEXT DEFAULT (datetime('now')), \
    modified_at TEXT DEFAULT (datetime('now')), \
    FOREIGN KEY (dlsite_tag_id) REFERENCES dlsite_tag(tag_id) ON DELETE CASCADE";

// Indexes pour file_processing
pub const DB_FILE_PROCESSING_INDEX_FLD_ID: &str =
    "CREATE INDEX IF NOT EXISTS idx_file_processing_fld_id ON file_processing(fld_id)";
pub const DB_FILE_PROCESSING_INDEX_TAG_DATE: &str =
    "CREATE INDEX IF NOT EXISTS idx_file_processing_tag_date ON file_processing(tag_date)";
