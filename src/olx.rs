//! Fetching and parsing OLX search result pages.

use anyhow::{Context, Result};
use scraper::{ElementRef, Html, Selector};

const BASE_URL: &str = "https://olx.pt";
const USER_AGENT: &str = concat!("olx-scraper/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub title: String,
    pub url: String,
    pub price: String,
    pub location: String,
    pub date: String,
    pub image_url: Option<String>,
}

pub fn fetch(agent: &ureq::Agent, url: &str) -> Result<Vec<Listing>> {
    let body = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("requesting {url}"))?
        .body_mut()
        .read_to_string()
        .with_context(|| format!("reading response from {url}"))?;

    Ok(parse(&body))
}

/// Parse a search result page. Pages that only have suggested results yield
/// no listings.
pub fn parse(html: &str) -> Vec<Listing> {
    let doc = Html::parse_document(html);
    if !has_results(&doc) {
        println!("    (only suggested listings, skipping)");
        return Vec::new();
    }
    extract(&doc)
}

fn extract(doc: &Html) -> Vec<Listing> {
    let card = selector("[data-testid=l-card]");
    let link = selector("a");
    let title = selector("h4");
    let price = selector("[data-testid=ad-price]");
    let location_date = selector("[data-testid=location-date]");
    let image = selector("img");

    doc.select(&card)
        .filter_map(|card| {
            let href = card.select(&link).next()?.value().attr("href")?;
            let (location, date) = match text_of(&card, &location_date) {
                Some(text) => split_location_date(&text),
                None => ("N/A".to_owned(), "N/A".to_owned()),
            };

            Some(Listing {
                title: text_of(&card, &title).unwrap_or_else(|| "No title".to_owned()),
                url: absolute_url(strip_query(href)),
                price: text_of(&card, &price)
                    .map(|text| clean_price(&text))
                    .unwrap_or_else(|| "N/A".to_owned()),
                location,
                date,
                image_url: card
                    .select(&image)
                    .next()
                    .and_then(|image| image.value().attr("src"))
                    .map(|src| absolute_url(clean_image_url(src))),
            })
        })
        .collect()
}

/// OLX renders an explicit count for real search results and a header message
/// for a page that only has suggestions.
fn has_results(doc: &Html) -> bool {
    if let Some(total) = doc.select(&selector("[data-testid=total-count]")).next() {
        return !total.text().collect::<String>().contains("0 anúncios");
    }
    doc.select(&selector("[data-testid=qa-header-message]"))
        .next()
        .is_none()
}

fn text_of(element: &ElementRef<'_>, selector: &Selector) -> Option<String> {
    element
        .select(selector)
        .next()
        .map(|found| found.text().collect::<String>().trim().to_owned())
}

/// OLX appends the price's inline CSS inside the same paragraph, and marks
/// negotiable prices with a trailing "Negociável".
fn clean_price(text: &str) -> String {
    let negotiable = text.contains("Negociável");
    let without_css = text.split_once(".css").map_or(text, |(head, _)| head);
    let clean = without_css
        .split_once("Negociável")
        .map_or(without_css, |(head, _)| head)
        .trim();

    if negotiable {
        format!("{clean} (Negociável)")
    } else {
        clean.to_owned()
    }
}

fn split_location_date(text: &str) -> (String, String) {
    match text.split_once(" - ") {
        Some((location, date)) => (location.trim().to_owned(), date.trim().to_owned()),
        None => (text.trim().to_owned(), "N/A".to_owned()),
    }
}

fn absolute_url(url: &str) -> String {
    if url.starts_with('/') {
        format!("{BASE_URL}{url}")
    } else {
        url.to_owned()
    }
}

fn strip_query(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

fn clean_image_url(url: &str) -> &str {
    url.split(';').next().unwrap_or(url)
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("valid CSS selector")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_card() {
        let listings = parse(include_str!("../tests/fixtures/card.html"));

        assert_eq!(listings.len(), 1);
        let listing = &listings[0];
        assert_eq!(listing.title, "Hyndai Ioniq eletrico 28kw");
        assert_eq!(listing.price, "9.700 €");
        assert_eq!(listing.location, "Gafanha Da Encarnação");
        assert_eq!(listing.date, "27 de outubro de 2026");
        assert_eq!(
            listing.url,
            "https://olx.pt/d/anuncio/hyndai-ioniq-eletrico-28kw-IDJBBvC.html"
        );
        assert_eq!(
            listing.image_url.as_deref(),
            Some("https://ireland.apollo.olxcdn.com:443/v1/files/ucf2wbsaer41-PT/image")
        );
    }

    #[test]
    fn cleans_prices() {
        assert_eq!(clean_price("9.700 €"), "9.700 €");
        assert_eq!(clean_price("9.700 € Negociável"), "9.700 € (Negociável)");
        assert_eq!(clean_price("9.700 €.css-x{color:red}"), "9.700 €");
        assert_eq!(
            clean_price("1.000 € Negociável.css-x{color:red}"),
            "1.000 € (Negociável)"
        );
    }

    #[test]
    fn splits_location_and_date() {
        assert_eq!(
            split_location_date("Lisboa - Hoje"),
            ("Lisboa".to_owned(), "Hoje".to_owned())
        );
        assert_eq!(
            split_location_date("Lisboa"),
            ("Lisboa".to_owned(), "N/A".to_owned())
        );
    }

    #[test]
    fn normalizes_urls() {
        assert_eq!(
            absolute_url(strip_query("/d/anuncio/one.html?search_reason=organic")),
            "https://olx.pt/d/anuncio/one.html"
        );
        assert_eq!(
            clean_image_url("https://cdn.example/image;s=216x152;q=50"),
            "https://cdn.example/image"
        );
    }

    #[test]
    fn skips_suggested_only_pages() {
        let doc = Html::parse_document(r#"<div data-testid="qa-header-message">Sugestões</div>"#);
        assert!(!has_results(&doc));
    }

    #[test]
    fn skips_zero_result_pages() {
        let doc = Html::parse_document(r#"<p data-testid="total-count">0 anúncios</p>"#);
        assert!(!has_results(&doc));
    }

    #[test]
    fn accepts_pages_with_results() {
        let doc = Html::parse_document(r#"<p data-testid="total-count">1 anúncio</p>"#);
        assert!(has_results(&doc));
    }
}
