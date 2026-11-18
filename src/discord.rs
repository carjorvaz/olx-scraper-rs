//! Discord webhook payloads.

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::olx::Listing;

const USER_AGENT: &str = concat!("olx-scraper/", env!("CARGO_PKG_VERSION"));

pub fn payload(listing: &Listing) -> Value {
    let mut embed = json!({
        "title": &listing.title,
        "url": &listing.url,
        "fields": [
            { "name": "Preço", "value": &listing.price },
            { "name": "Localização", "value": &listing.location },
            { "name": "Data", "value": &listing.date },
        ],
    });
    if let Some(image_url) = &listing.image_url {
        embed["image"] = json!({ "url": image_url });
    }
    json!({ "embeds": [embed] })
}

pub fn send(agent: &ureq::Agent, webhook: &str, listing: &Listing) -> Result<()> {
    agent
        .post(webhook)
        .header("User-Agent", USER_AGENT)
        .send_json(payload(listing))
        .context("posting to Discord")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing() -> Listing {
        Listing {
            title: "Hyndai Ioniq eletrico 28kw".to_owned(),
            url: "https://olx.pt/d/anuncio/one.html".to_owned(),
            price: "9.700 €".to_owned(),
            location: "Gafanha Da Encarnação".to_owned(),
            date: "27 de outubro de 2026".to_owned(),
            image_url: Some("https://cdn.example/image".to_owned()),
        }
    }

    #[test]
    fn builds_an_embed() {
        let payload = payload(&listing());

        assert_eq!(payload["embeds"][0]["title"], "Hyndai Ioniq eletrico 28kw");
        assert_eq!(
            payload["embeds"][0]["url"],
            "https://olx.pt/d/anuncio/one.html"
        );
        assert_eq!(payload["embeds"][0]["fields"][0]["name"], "Preço");
        assert_eq!(payload["embeds"][0]["fields"][0]["value"], "9.700 €");
        assert_eq!(
            payload["embeds"][0]["image"]["url"],
            "https://cdn.example/image"
        );
    }

    #[test]
    fn omits_a_missing_image() {
        let mut listing = listing();
        listing.image_url = None;

        let payload = payload(&listing);

        assert!(payload["embeds"][0].get("image").is_none());
    }
}
