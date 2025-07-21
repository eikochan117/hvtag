use serde::{Deserialize, Serialize};
use serde_json::{Value};

#[derive(Serialize, Deserialize, Debug)]
pub struct RankEntry {
    pub term: String,
    pub category: String,
    pub rank: u32,
    pub rank_date: String
}
#[derive(Serialize, Deserialize, Debug)]

pub struct ReviewEntry {
    pub review_point: u32,
    pub count: u32,
    pub ratio: u32
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TranslationInfoEntry {
    pub is_translation_agree: bool,
    pub is_volunteer: bool,
    pub is_original: bool,
    pub is_parent: bool,
    pub is_child: bool,
    pub is_translation_bonus_child: bool,
    pub original_workno: Option<String>,
    pub parent_workno: Option<String>,
    pub child_worknos: Vec<String>,
    pub lang: Option<String>,
    pub production_trade_price_rate: u32,
    //pub translation_bonus_langs: Vec<String>
    #[serde(flatten)]
    pub extra: Option<Value>
}
#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct LocalePriceEntry {
    pub en_US: f32,
    pub ar_AE: f32,
    pub es_ES: f32,
    pub de_DE: f32,
    pub fr_FR: f32,
    pub it_IT: f32,
    pub pt_BR: f32,
    pub zh_TW: f32,
    pub zh_CN: f32,
    pub ko_KR: u32,
    pub id_ID: u32,
    pub vi_VN: u32,
    pub th_TH: f32,
    pub sv_SE: f32
}
#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct LocalePriceStrEntry {
    pub en_US: String,
    pub ar_AE: String,
    pub es_ES: String,
    pub de_DE: String,
    pub fr_FR: String,
    pub it_IT: String,
    pub pt_BR: String,
    pub zh_TW: String,
    pub zh_CN: String,
    pub ko_KR: String,
    pub id_ID: String,
    pub vi_VN: String,
    pub th_TH: String,
    pub sv_SE: String
}
#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct CurrencyPriceEntry {
    pub JPY: u32,
    pub USD: f32,
    pub EUR: f32,
    pub GBP: f32,
    pub TWD: f32,
    pub CNY: f32,
    pub KRW: f32,
    pub IDR: f32,
    pub VND: f32,
    pub THB: f32,
    pub SEK: f32,
    pub HKD: f32,
    pub SGD: f32,
    pub CAD: f32,
    pub MYR: f32,
    pub BRL: f32,
    pub AUD: f32,
    pub PHP: f32,
    pub MXN: f32,
    pub NZD: f32,
    pub INR: f32
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum StringOrU32 {
    String(String),
    U32(u32),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DlCountItemEntry {
    pub workno: String,
    pub edition_id: u32,
    pub edition_type: String,
    pub display_order: u32,
    pub label: String,
    pub lang: String,
    pub dl_count: StringOrU32,
    pub display_label: String
}
#[derive(Serialize, Deserialize, Debug)]
pub struct DlSiteProductIdResult {
    pub site_id: String,
    pub site_id_touch: String,
    pub maker_id: String,
    pub age_category: u32,
    pub affiliate_deny: u32,
    pub dl_count: u32,
    pub wishlist_count: u32,
    pub dl_format: u32,
    pub rank: Vec<RankEntry>,
    pub rate_average: u32,
    pub rate_average_2dp: f32,
    pub rate_average_star: u32,
    pub rate_count: u32,
    pub rate_count_detail: Vec<ReviewEntry>,
    pub review_count: u32,
    pub price: u32,
    pub price_without_tax: u32,
    pub price_str: String,
    pub default_point_rate: u32,
    pub default_point: u32,
    pub product_point_rate: Option<u32>,
    pub dlsiteplay_work: bool,
    pub is_ana: bool,
    pub is_sale: bool,
    pub is_discount: bool,
    pub is_pointup: bool,
    pub gift: Vec<String>,
    pub is_rental: bool,
    pub work_rentals: Vec<String>,
    pub upgrade_min_price: u32,
    pub down_url: String,
    pub is_target: Option<bool>,
    pub title_id: Option<String>,
    pub title_name: Option<String>,
    pub title_name_masked: Option<String>,
    pub title_volumn: Option<u32>,
    pub title_work_count: Option<u32>,
    pub is_title_completed: bool,
    pub bulkbuy_key: Option<String>,
    pub bonuses: Vec<String>,
    pub is_limit_work: bool,
    pub is_sold_out: bool,
    pub limit_stock: u32,
    pub is_reserve_work: bool,
    pub is_reservable: bool,
    pub is_timesale: bool,
    pub timesale_stock: u32,
    pub is_free: bool,
    pub is_oly: bool,
    pub is_led: bool,
    pub is_noreduction: bool,
    pub is_wcc: bool,
    pub translation_info: TranslationInfoEntry,
    pub work_name: String,
    pub work_name_masked: String,
    pub work_image: String,
    pub sales_end_info: Option<String>,
    pub voice_pack: Option<String>,
    pub regist_date: String,
    pub locale_price: LocalePriceEntry,
    pub locale_price_str: LocalePriceStrEntry,
    pub currency_price: CurrencyPriceEntry,
    pub work_type: String,
    pub book_type: Option<String>,
    pub discount_calc_type: Option<String>,
    pub is_pack_work: bool,
    pub limited_free_terms: Vec<String>,
    pub official_price: u32,
    pub options: String,
    pub custom_genres: Vec<String>,
    pub dl_count_total: u32,
    #[serde(skip_serializing)]
    pub dl_count_items: Vec<DlCountItemEntry>,
    pub default_point_str: String
}
