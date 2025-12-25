use std::fmt::Display;

use crate::dlsite::types::DlSiteProductIdResult;

#[derive(Debug)]
pub enum AgeCategory {
    Other = 0,
    AllAge = 1,
    R15 = 2,
    R18 = 3
}

impl AgeCategory {
    pub fn from_int(i: u32) -> Self {
        match i {
            1 => AgeCategory::AllAge,
            3 => AgeCategory::R18,
            _ => AgeCategory::Other
        }
    }
}

impl Default for AgeCategory {
    fn default() -> Self {
        Self::R18
    }
}

impl Display for AgeCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgeCategory::Other => write!(f, "Other"),
            AgeCategory::AllAge => write!(f, "All Ages"),
            AgeCategory::R15 => write!(f, "R15"),
            AgeCategory::R18 => write!(f, "R18"),
        }
    }
}

#[derive(Default, Debug)]
pub struct WorkDetails {
    pub rjcode: String,
    pub maker_code: crate::folders::types::RGCode,
    pub age_category: AgeCategory,
    pub rate: f32,
    pub name: String,
    pub image_link: String,
    pub release_date: String,
}

impl WorkDetails {
    pub fn from_dlsite_product_id_result(rjcode: &str, p: DlSiteProductIdResult) -> Self {
        WorkDetails {
            rjcode: rjcode.to_string(),
            maker_code: crate::folders::types::RGCode::new(p.maker_id),
            age_category: AgeCategory::from_int(p.age_category),
            rate: p.rate_average_2dp,
            name: p.work_name,
            image_link: p.work_image,
            release_date: p.regist_date,
        }
    }
}

pub struct Work {
    rjcode: String,
    name: String,
    age_category: AgeCategory,
    circle_name : String,
    circle_code: String,
    image_link: String,
    release_date: String,
    seiyuu: Vec<String>,
    tags: Vec<String>,
}

// Audio tagging types for Step 3

#[derive(Debug, Clone)]
pub struct AudioMetadata {
    pub title: String,              // work name
    pub artists: Vec<String>,       // voice actors (CVs) - can be multiple
    pub album: String,              // work name
    pub album_artist: String,       // circle name
    pub track_number: Option<u32>,  // parsed from filename
    pub genre: Vec<String>,         // dlsite tags
    pub date: Option<String>,       // release_date
    // Note: Cover art is NOT in AudioMetadata - it's saved separately as folder.jpeg
}

#[derive(Debug, Clone)]
pub struct TaggerConfig {
    pub convert_to_mp3: bool,
    pub target_bitrate: u32,
    pub download_cover: bool,
    pub cover_size: (u32, u32),
}

impl Default for TaggerConfig {
    fn default() -> Self {
        TaggerConfig {
            convert_to_mp3: false,
            target_bitrate: 320,
            download_cover: true,
            cover_size: (300, 300),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum AudioFormat {
    Mp3,
    Flac,
    Wav,
    Ogg,
    Unknown,
}

impl AudioFormat {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "mp3" => AudioFormat::Mp3,
            "flac" => AudioFormat::Flac,
            "wav" => AudioFormat::Wav,
            "ogg" => AudioFormat::Ogg,
            _ => AudioFormat::Unknown,
        }
    }
}
