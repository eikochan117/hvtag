use reqwest::Url;
use scraper::{ElementRef, Html, Selector};
use tracing::warn;
use crate::{errors::HvtError, folders::types::RJCode};

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

/// Fallback CV extraction for works (common in R18/ASMR listings) that credit the voice actor
/// only inside the free-text `[Staff]` block of the work description (`.work_parts_area`),
/// never in the structured product-info table. Each `<br/>`-separated line becomes its own
/// text node when iterating `ElementRef::text()`, so a line-by-line scan for a
/// `CV:`/`CV：`/`声優:`/`声優：` prefix reliably isolates just the credit line without needing
/// to parse the raw `<br/>` markup.
fn extract_cv_from_staff_block(html: &str) -> Result<Vec<String>, HvtError> {
    const CV_LINE_PREFIXES: [&str; 4] = ["CV:", "CV：", "声優:", "声優："];

    let document = Html::parse_document(html);
    let selector = Selector::parse(".work_parts_area")
        .map_err(|e| HvtError::Parse(format!("Failed to parse work_parts_area selector: {:?}", e)))?;

    for container in document.select(&selector) {
        for text_node in container.text() {
            let line = text_node.trim();
            for prefix in CV_LINE_PREFIXES {
                if let Some(rest) = line.strip_prefix(prefix) {
                    let names: Vec<String> = rest
                        .split(|c| c == '/' || c == '、' || c == '&')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !names.is_empty() {
                        return Ok(names);
                    }
                }
            }
        }
    }

    Ok(vec![])
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
        let code = RJCode::from_string_unchecked(rjcode.clone());
        let section = code.site_section();
        let url_str = format!("https://www.dlsite.com/{section}/work/=/product_id/{rjcode}.html");
        let url = url_str.parse::<Url>()
            .map_err(|e| HvtError::Http(format!("Invalid URL: {}", e)))?;

        let default_client = reqwest::Client::new();
        let http_client = client.unwrap_or(&default_client);

        let resp = http_client
            .get(url)
            .header("Cookie", "locale=en_US")
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

        // Extract CVs - Try English FIRST (since we're using en_US locale), then Japanese as fallback
        let mut cvs = vec![];
        if let Some(elem) = extract_td_after_th(&html, "Voice Actor")? {
            cvs = elem.split(" / ").map(|x| x.trim().to_string()).collect();
        }
        if cvs.is_empty() {
            if let Some(elem) = extract_td_after_th(&html, "声優")? {
                cvs = elem.split(" / ").map(|x| x.trim().to_string()).collect();
            }
        }
        if cvs.is_empty() {
            cvs = extract_cv_from_staff_block(&html)?;
        }
        if cvs.is_empty() {
            cvs.push(String::from("<unknown>"));
        }

        // Extract BOTH circle names (EN and JP)
        // Since we're using en_US locale, try English first
        let circle_name_en = extract_td_after_th(&html, "Circle")?.map(|s| s.trim().to_string());
        let circle_name_jp = extract_td_after_th(&html, "サークル名")?.map(|s| s.trim().to_string());

        // For backward compatibility, set circle_name to EN if available, else JP (since we're in EN locale)
        let circle_name = circle_name_en.clone().or(circle_name_jp.clone());

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

/// Scrape circle names from circle profile page TITLE.
/// Makes 2 requests with different locales to get both EN and JP names.
///
/// `section` should be `"maniax"` (RJ works) or `"pro"` (VJ works).
/// Returns (name_en, name_jp)
pub async fn scrape_circle_profile(
    rgcode: &str,
    section: &str,
    client: Option<&reqwest::Client>,
) -> Result<(String, String), HvtError> {
    let subpath = if section == "pro" { "maker/profile" } else { "circle/profile" };
    let url_str = format!("https://www.dlsite.com/{section}/{subpath}/=/maker_id/{rgcode}.html");
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the real structure found on RJ197417's page: no structured Voice Actor row,
    /// CV credited only in the free-text [Staff] block inside .work_parts_area.
    #[test]
    fn test_extract_cv_from_staff_block_english() {
        let html = r#"<html><body>
            <div class="work_parts_area">
                <p>Some description text.<br />
                <br />
                [Staff]<br />
                CV: Nodoka Nishiura<br />
                Illustration: tegurayuki<br />
                Scenario: Chitatsu Omi</p>
            </div>
        </body></html>"#;

        let cvs = extract_cv_from_staff_block(html).unwrap();
        assert_eq!(cvs, vec!["Nodoka Nishiura".to_string()]);
    }

    #[test]
    fn test_extract_cv_from_staff_block_japanese_fullwidth_colon() {
        let html = r#"<html><body>
            <div class="work_parts_area">
                <p>[Staff]<br />
                声優：花子<br />
                イラスト：太郎</p>
            </div>
        </body></html>"#;

        let cvs = extract_cv_from_staff_block(html).unwrap();
        assert_eq!(cvs, vec!["花子".to_string()]);
    }

    #[test]
    fn test_extract_cv_from_staff_block_multiple_names() {
        let html = r#"<html><body>
            <div class="work_parts_area">
                <p>[Staff]<br />
                CV: Name A / Name B</p>
            </div>
        </body></html>"#;

        let cvs = extract_cv_from_staff_block(html).unwrap();
        assert_eq!(cvs, vec!["Name A".to_string(), "Name B".to_string()]);
    }

    #[test]
    fn test_extract_cv_from_staff_block_no_credit_present() {
        let html = r#"<html><body>
            <div class="work_parts_area">
                <p>Just a description with no staff credits at all.</p>
            </div>
        </body></html>"#;

        let cvs = extract_cv_from_staff_block(html).unwrap();
        assert!(cvs.is_empty());
    }

    #[test]
    fn test_extract_cv_from_staff_block_no_container_present() {
        let html = r#"<html><body><p>No work_parts_area div at all.</p></body></html>"#;
        let cvs = extract_cv_from_staff_block(html).unwrap();
        assert!(cvs.is_empty());
    }
}
