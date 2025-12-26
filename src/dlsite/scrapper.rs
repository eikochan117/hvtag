use reqwest::Url;
use scraper::{ElementRef, Html, Selector};
use tracing::warn;
use crate::errors::HvtError;

#[derive(Debug)]
pub struct DlSiteProductScrapResult {
    pub genre: Vec<String>,
    pub cvs: Vec<String>,
    pub circle_name: Option<String>,      // Backward compat (JP if avail, else EN)
    pub circle_name_en: Option<String>,   // English circle name
    pub circle_name_jp: Option<String>,   // Japanese circle name
}

fn extract_td_after_th(html: &str, th_text: &str) -> Result<Option<String>, HvtError> {
    let document = Html::parse_document(html);

    let th_selector = Selector::parse("th")
        .map_err(|e| HvtError::Parse(format!("Failed to parse th selector: {:?}", e)))?;
    let td_selector = Selector::parse("td")
        .map_err(|e| HvtError::Parse(format!("Failed to parse td selector: {:?}", e)))?;

    for th_element in document.select(&th_selector) {
        if th_element.text().collect::<Vec<_>>().join("").trim() == th_text {
            if let Some(parent_node) = th_element.parent() {
                if let Some(parent_element) = ElementRef::wrap(parent_node) {
                    if let Some(td) = parent_element.select(&td_selector).next() {
                        return Ok(Some(td.text().collect::<Vec<_>>().join("").trim().to_string()));
                    }
                }
            }
        }
    }
    Ok(None)
}

impl DlSiteProductScrapResult {
    pub async fn build_from_rjcode(rjcode: String) -> DlSiteProductScrapResult {
        Self::build_from_rjcode_with_client(rjcode, None).await
    }

    pub async fn build_from_rjcode_with_client(
        rjcode: String,
        client: Option<&reqwest::Client>,
    ) -> DlSiteProductScrapResult {
        // Internal function that handles errors - converts them to default values
        match Self::build_from_rjcode_impl(rjcode, client).await {
            Ok(result) => result,
            Err(e) => {
                warn!("Failed to scrape DLSite data: {}", e);
                // Return empty result on error (will be detected as RemovedWork)
                DlSiteProductScrapResult {
                    genre: vec![],
                    cvs: vec![String::from("<unknown>")],
                    circle_name: None,
                    circle_name_en: None,
                    circle_name_jp: None,
                }
            }
        }
    }

    async fn build_from_rjcode_impl(
        rjcode: String,
        client: Option<&reqwest::Client>,
    ) -> Result<DlSiteProductScrapResult, HvtError> {
        let url_str = format!("https://www.dlsite.com/maniax/work/=/product_id/{rjcode}.html");
        let url = url_str.parse::<Url>()
            .map_err(|e| HvtError::Http(format!("Invalid URL: {}", e)))?;

        let default_client = reqwest::Client::new();
        let http_client = client.unwrap_or(&default_client);

        let resp = http_client
            .get(url)
            .header("Cookie", "locale=jp_JP")
            .header("Accept-Language", "en-US")
            .send()
            .await
            .map_err(|e| HvtError::Http(format!("HTTP request failed: {}", e)))?;

        let html = resp.text().await
            .map_err(|e| HvtError::Http(format!("Failed to get response text: {}", e)))?;

        let document = Html::parse_document(&html);
        let selector = Selector::parse(".main_genre")
            .map_err(|e| HvtError::Parse(format!("Failed to parse main_genre selector: {:?}", e)))?;

        let mut genre = vec![];
        if let Some(elem) = document.select(&selector).next() {
            let content = elem.text().filter(|x| !x.contains("\n")).collect::<Vec<_>>();
            for c in content {
                genre.push(c.replace("'", "''").to_string());
            }
        }

        // Extract CVs - Try Japanese FIRST, then English
        let mut cvs = vec![];
        if let Some(elem) = extract_td_after_th(&html, "声優")? {
            cvs = elem.split(" / ").map(|x| x.trim().to_string()).collect();
        }
        if cvs.is_empty() {
            if let Some(elem) = extract_td_after_th(&html, "Voice Actor")? {
                cvs = elem.split(" / ").map(|x| x.trim().to_string()).collect();
            }
        }
        if cvs.is_empty() {
            cvs.push(String::from("<unknown>"));
        }

        // Extract BOTH circle names (EN and JP)
        let circle_name_en = extract_td_after_th(&html, "Circle")?.map(|s| s.trim().to_string());
        let circle_name_jp = extract_td_after_th(&html, "サークル名")?.map(|s| s.trim().to_string());

        // For backward compatibility, set circle_name to JP if available, else EN
        let circle_name = circle_name_jp.clone().or(circle_name_en.clone());

        Ok(DlSiteProductScrapResult {
            genre,
            cvs,
            circle_name,        // JP prioritaire (backward compat)
            circle_name_en,     // English name
            circle_name_jp,     // Japanese name
        })
    }
}

/// Parse circle name from page title
/// Title format: "Circle Name（カタカナ） Circle Profile | ..."
/// Extracts only the name before the katakana pronunciation
fn parse_circle_name_from_title(title: &str) -> String {
    let title = title.trim();

    // Remove everything after " Circle Profile" or " サークルプロフィール"
    let name = title
        .split(" Circle Profile")
        .next()
        .unwrap_or(title)
        .split(" サークルプロフィール")
        .next()
        .unwrap_or(title);

    // Remove katakana pronunciation in Japanese parentheses （...） if present
    let name = name.split('（').next().unwrap_or(name);

    name.trim().to_string()
}

/// Scrape circle names from circle profile page TITLE
/// URL: https://www.dlsite.com/maniax/circle/profile/=/maker_id/<RG Code>.html
/// Makes 2 requests with different locales to get both EN and JP names
///
/// Returns (name_en, name_jp)
pub async fn scrape_circle_profile(
    rgcode: &str,
    client: Option<&reqwest::Client>,
) -> Result<(String, String), HvtError> {
    let url_str = format!("https://www.dlsite.com/maniax/circle/profile/=/maker_id/{}.html", rgcode);
    let url = url_str.parse::<Url>()
        .map_err(|e| HvtError::Http(format!("Invalid URL: {}", e)))?;

    let default_client = reqwest::Client::new();
    let http_client = client.unwrap_or(&default_client);

    let title_selector = Selector::parse("title")
        .map_err(|e| HvtError::Parse(format!("Failed to parse title selector: {:?}", e)))?;

    // Request 1: Get EN name with locale=en_US
    let resp_en = http_client
        .get(url.clone())
        .header("Cookie", "locale=en_US")
        .header("Accept-Language", "en-US")
        .send()
        .await
        .map_err(|e| HvtError::Http(format!("HTTP request failed (EN): {}", e)))?;

    let html_en = resp_en.text().await
        .map_err(|e| HvtError::Http(format!("Failed to get response text (EN): {}", e)))?;

    let document_en = Html::parse_document(&html_en);
    let name_en = if let Some(title_elem) = document_en.select(&title_selector).next() {
        let title_text = title_elem.text().collect::<Vec<_>>().join("").trim().to_string();
        parse_circle_name_from_title(&title_text)
    } else {
        return Err(HvtError::Parse("No title tag found in circle profile page (EN)".to_string()));
    };

    // Request 2: Get JP name with locale=ja_JP
    let resp_jp = http_client
        .get(url)
        .header("Cookie", "locale=ja_JP")
        .header("Accept-Language", "ja-JP")
        .send()
        .await
        .map_err(|e| HvtError::Http(format!("HTTP request failed (JP): {}", e)))?;

    let html_jp = resp_jp.text().await
        .map_err(|e| HvtError::Http(format!("Failed to get response text (JP): {}", e)))?;

    let document_jp = Html::parse_document(&html_jp);
    let name_jp = if let Some(title_elem) = document_jp.select(&title_selector).next() {
        let title_text = title_elem.text().collect::<Vec<_>>().join("").trim().to_string();
        parse_circle_name_from_title(&title_text)
    } else {
        return Err(HvtError::Parse("No title tag found in circle profile page (JP)".to_string()));
    };

    Ok((name_en, name_jp))
}
