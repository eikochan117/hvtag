use std::error::Error;
use tracing::debug;

use crate::{folders::types::RGCode, tagger::types::{AgeCategory, WorkDetails}};

impl WorkDetails {
    pub async fn build_from_rjcode(rjcode: String) -> Result<Self, Box<dyn Error>> {
        Self::build_from_rjcode_with_client(rjcode, None).await
    }

    pub async fn build_from_rjcode_with_client(
        rjcode: String,
        client: Option<&reqwest::Client>,
    ) -> Result<Self, Box<dyn Error>> {
        let url = format!("https://www.dlsite.com/maniax/product/info/ajax?product_id={rjcode}");
        debug!("Querying DLSite API: {url}");

        let resp = if let Some(client) = client {
            client.get(&url).send().await?.text().await?
        } else {
            reqwest::get(&url).await?.text().await?
        };

        // Parse as generic Value to avoid type mismatches with variable DLSite API fields.
        // DLSite also migrated old 6-digit codes (e.g. RJ584634) to 8-digit format (e.g. RJ01584634)
        // by adding "01" prefix — the API may return the old key when queried with the new one.
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str::<serde_json::Value>(&resp)?
            .as_object()
            .cloned()
            .ok_or("DLSite API response is not a JSON object")?;

        let work = if let Some(v) = map.get(&rjcode) {
            v.clone()
        } else if map.len() == 1 {
            map.into_values().next().unwrap()
        } else {
            return Err(format!("DLSite API returned unexpected response for {rjcode}").into());
        };

        let maker_id = work["maker_id"].as_str().unwrap_or("").to_string();
        let age_category = work["age_category"].as_u64().unwrap_or(0) as u32;
        let rate = work["rate_average_2dp"].as_f64().unwrap_or(0.0) as f32;
        let name = work["work_name"].as_str().unwrap_or("").to_string();
        let work_image = work["work_image"].as_str().unwrap_or("").to_string();
        let release_date = work["regist_date"].as_str().unwrap_or("").to_string();

        let image_link = if work_image.starts_with("//") {
            format!("https:{work_image}")
        } else {
            work_image
        };

        Ok(WorkDetails {
            rjcode,
            maker_code: RGCode::new(maker_id),
            age_category: AgeCategory::from_int(age_category),
            rate,
            name,
            image_link,
            release_date,
        })
    }
}
