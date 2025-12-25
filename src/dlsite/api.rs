use std::{collections::HashMap, error::Error};
use tracing::debug;

use crate::tagger::types::WorkDetails;

use super::types::DlSiteProductIdResult;

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

        let mut json : HashMap<String, DlSiteProductIdResult> = serde_json::from_str(&resp)?;
        let json = json.remove(&rjcode).expect("result from Dlsite was different");
        let res = WorkDetails::from_dlsite_product_id_result(&rjcode, json);
        Ok(res)
    }
}
