//! A small OLX search scraper that posts new listings to Discord webhooks.

mod config;
mod discord;
mod olx;
mod state;

use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Error, Result, anyhow};

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const POST_DELAY: Duration = Duration::from_secs(2);

pub fn run() -> Result<()> {
    let config_file =
        env::var("CONFIG_FILE").context("CONFIG_FILE environment variable is required")?;
    let state_dir = env::var("STATE_DIR").map_or_else(|_| PathBuf::from("."), PathBuf::from);
    let seed_only = env::var("SEED_ONLY").is_ok_and(|value| is_true(&value));

    let groups = config::load(Path::new(&config_file))?;
    let mut seen = state::load(&state_dir)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();

    println!("Loaded {} webhook/URL groups", groups.len());
    if seed_only {
        println!("SEED_ONLY set: listings will be recorded, nothing will be sent");
    }

    let mut total_new = 0;
    let mut first_error: Option<Error> = None;
    let total_urls: usize = groups.iter().map(|group| group.urls.len()).sum();
    let mut scrape_failures: usize = 0;
    let mut drifted: usize = 0;

    for group in &groups {
        println!("\nProcessing group with {} search URLs", group.urls.len());

        'urls: for url in &group.urls {
            println!("  Scraping: {url}");
            let page = match olx::fetch(&agent, url) {
                Ok(page) => page,
                Err(error) => {
                    scrape_failures += 1;
                    eprintln!("Error scraping {url}: {error:#}");
                    continue;
                }
            };
            if page.drifted {
                drifted += 1;
            }
            let listings = page.listings;
            let new = new_listings(&listings, &seen);
            println!("    Found {} listings, {} new", listings.len(), new.len());

            for listing in new {
                if seed_only {
                    println!("    Marking seen: {}", listing.title);
                } else {
                    println!("    Sending: {}", listing.title);
                    if let Err(error) = discord::send(&agent, &group.webhook, listing) {
                        eprintln!("Error sending {}: {error:#}", listing.title);
                        first_error.get_or_insert(error);
                        break 'urls;
                    }
                    thread::sleep(POST_DELAY);
                }
                seen.insert(listing.url.clone());
                total_new += 1;
            }
        }

        state::save(&state_dir, &seen)?;
    }

    if let Some(error) = first_error {
        return Err(error.context(format!("after sending {total_new} new listings")));
    }

    let mut problems = Vec::new();
    if drifted > 0 {
        problems.push(format!(
            "{drifted} page(s) reported results but no cards; the selectors may have changed"
        ));
    }
    if total_urls > 0 && scrape_failures == total_urls {
        problems.push(format!(
            "all searches failed ({scrape_failures}/{total_urls})"
        ));
    }
    if !problems.is_empty() {
        return Err(anyhow!("{}", problems.join("; ")));
    }

    let action = if seed_only {
        "Marked seen (seed only)"
    } else {
        "Sent"
    };
    println!("\nDone. {action} {total_new} new listings total.");
    Ok(())
}

fn is_true(value: &str) -> bool {
    !value.is_empty() && !value.eq_ignore_ascii_case("0") && !value.eq_ignore_ascii_case("false")
}

/// Listings that are neither already seen nor repeated within the same page.
fn new_listings<'a>(listings: &'a [olx::Listing], seen: &HashSet<String>) -> Vec<&'a olx::Listing> {
    let mut page_urls = HashSet::new();
    let mut new = Vec::new();
    for listing in listings {
        if !seen.contains(&listing.url) && page_urls.insert(listing.url.as_str()) {
            new.push(listing);
        }
    }
    new
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(url: &str) -> olx::Listing {
        olx::Listing {
            title: url.to_owned(),
            url: url.to_owned(),
            price: String::new(),
            location: String::new(),
            date: String::new(),
            image_url: None,
        }
    }

    #[test]
    fn skips_seen_and_in_page_duplicates() {
        let listings = vec![listing("a"), listing("b"), listing("a"), listing("c")];
        let seen = HashSet::from(["b".to_owned()]);

        let urls: Vec<&str> = new_listings(&listings, &seen)
            .into_iter()
            .map(|listing| listing.url.as_str())
            .collect();

        assert_eq!(urls, ["a", "c"]);
    }
}
