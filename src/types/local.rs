use super::dlsite::DlSiteProductIdResult;
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

#[derive(Default, Debug)]
pub struct WorkDetails {
    pub rjcode: String,
    pub maker_code: String,
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
            maker_code: p.maker_id,
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