use reqwest::Url;
use scraper::{ElementRef, Html, Selector};

#[derive(Debug)]
pub struct DlSiteProductScrapResult {
    pub genre: Vec<String>,
    pub cvs: Vec<String>
}

fn extract_td_after_th(html: &str, th_text: &str) -> Option<String> {
    let document = Html::parse_document(html);
    
    let th_selector = Selector::parse("th").unwrap();
    let td_selector = Selector::parse("td").unwrap();
    
    for th_element in document.select(&th_selector) {
        if th_element.text().collect::<Vec<_>>().join("").trim() == th_text {
            if let Some(parent_node) = th_element.parent() {
                if let Some(parent_element) = ElementRef::wrap(parent_node) {
                    if let Some(td) = parent_element.select(&td_selector).next() {
                        return Some(td.text().collect::<Vec<_>>().join("").trim().to_string());
                    }
                }
            }
        }
    }
    None
}

impl DlSiteProductScrapResult {
    pub async fn build_from_rjcode(rjcode: String) -> DlSiteProductScrapResult {
        let url = format!("https://www.dlsite.com/maniax/work/=/product_id/{rjcode}.html").parse::<Url>().unwrap();
        // let resp = reqwest::get(&url).awai   t.unwrap();
        let resp = reqwest::Client::new()
            .get(url)
            .header("Cookie", "locale=en_US")
            .header("Accept-Language", "en-US")
            .send()
            .await
            .unwrap();
        let html = resp.text().await.unwrap();

        let document = Html::parse_document(&html);
        let selector = Selector::parse(".main_genre").unwrap();

        let mut genre = vec![];
        if let Some(elem) = document.select(&selector).next() {
            let content = elem.text().filter(|x| !x.contains("\n")).collect::<Vec<_>>();
            for c in content {
                genre.push(c.to_string());
            }

        }

        let mut cvs = vec![];
        if let Some(elem) = extract_td_after_th(&html, "Voice Actor") {
            cvs = elem.split("/").map(|x| x.to_string()).collect();
        }

        DlSiteProductScrapResult {
            genre,
            cvs
        }
    }
}
