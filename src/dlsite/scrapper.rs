use reqwest::Url;
use scraper::{Html, Selector};

#[derive(Debug)]
pub struct DlSiteProductScrapResult {
    pub genre: Vec<String>
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

            DlSiteProductScrapResult {
                genre
            }
        } else {
            todo!();
        }
    }
}
