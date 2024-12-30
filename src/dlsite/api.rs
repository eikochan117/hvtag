use std::{collections::HashMap, error::Error};

use crate::types::{dlsite::DlSiteProductIdResult, local::WorkDetails};

impl WorkDetails {
    pub async fn build_from_rjcode(rjcode: String) -> Result<Self, Box<dyn Error>> {
        let url = format!("https://www.dlsite.com/maniax/product/info/ajax?product_id={rjcode}");
        let resp = reqwest::get(url).await?.text().await?;
        let mut json : HashMap<String, DlSiteProductIdResult> = serde_json::from_str(&resp)?;
        let json = json.remove(&rjcode).expect("result from Dlsite was different");
        let res = WorkDetails::from_dlsite_product_id_result(&rjcode, json);
        Ok(res)
    }
}