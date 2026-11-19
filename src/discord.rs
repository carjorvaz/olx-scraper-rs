//! Discord webhook payloads.

use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

use crate::olx::Listing;

const USER_AGENT: &str = concat!("olx-scraper/", env!("CARGO_PKG_VERSION"));

// Discord's limits for an embed title and a field value. A listing that
// exceeds them would be rejected forever, so clip instead of aborting.
const MAX_TITLE: usize = 256;
const MAX_FIELD: usize = 1024;

const MAX_ATTEMPTS: usize = 3;
const RATE_LIMIT_FALLBACK: Duration = Duration::from_secs(5);
const RATE_LIMIT_MARGIN: Duration = Duration::from_millis(500);

pub fn payload(listing: &Listing) -> Value {
    let mut embed = json!({
        "title": clip(&listing.title, MAX_TITLE),
        "url": &listing.url,
        "fields": [
            { "name": "Preço", "value": clip(&listing.price, MAX_FIELD) },
            { "name": "Localização", "value": clip(&listing.location, MAX_FIELD) },
            { "name": "Data", "value": clip(&listing.date, MAX_FIELD) },
        ],
    });
    if let Some(image_url) = &listing.image_url {
        embed["image"] = json!({ "url": image_url });
    }
    json!({ "embeds": [embed] })
}

pub fn send(agent: &ureq::Agent, webhook: &str, listing: &Listing) -> Result<()> {
    for attempt in 1..=MAX_ATTEMPTS {
        let response = agent
            .post(webhook)
            .header("User-Agent", USER_AGENT)
            .send_json(payload(listing))
            .map_err(|error| {
                anyhow!(
                    "posting to Discord: {}",
                    redact(&error.to_string(), webhook)
                )
            })?;

        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        let code = status.as_u16();
        if code == 429 && attempt < MAX_ATTEMPTS {
            let delay = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                .map(|seconds| Duration::from_secs_f64(seconds.min(60.0)))
                .unwrap_or(RATE_LIMIT_FALLBACK);
            eprintln!("Discord rate limited; retrying in {delay:?}");
            thread::sleep(delay + RATE_LIMIT_MARGIN);
            continue;
        }

        return Err(anyhow!("posting to Discord: HTTP {code}"));
    }

    unreachable!("the loop returns on success, a non-429 error, or its last attempt")
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(max - 1).collect();
    clipped.push('…');
    clipped
}

/// The webhook URL contains a token; never let it reach the logs.
fn redact(message: &str, webhook: &str) -> String {
    match webhook.rsplit('/').next() {
        Some(token) if !token.is_empty() => message.replace(token, "<token>"),
        _ => message.to_owned(),
    }
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

    #[test]
    fn clips_long_text() {
        let long = "x".repeat(300);
        let clipped = clip(&long, MAX_TITLE);

        assert_eq!(clipped.chars().count(), MAX_TITLE);
        assert!(clipped.ends_with('…'));
        assert_eq!(clip("short", MAX_TITLE), "short");
    }

    #[test]
    fn redacts_the_webhook_token() {
        let webhook = "https://discord.com/api/webhooks/1/s3cr3t";
        let message = format!("bad uri: {webhook} is missing host");

        let redacted = redact(&message, webhook);

        assert!(redacted.contains("<token>"));
        assert!(!redacted.contains("s3cr3t"));
    }
}
