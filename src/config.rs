//! The scraper config: groups of a Discord webhook followed by OLX search URLs.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub webhook: String,
    pub urls: Vec<String>,
}

pub fn load(path: &Path) -> Result<Vec<Group>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("reading config {}", path.display()))?;
    Ok(parse(&text))
}

/// Parse groups separated by blank lines. The first line of a group is the
/// Discord webhook; the remaining lines are OLX search URLs.
pub fn parse(text: &str) -> Vec<Group> {
    let mut groups = Vec::new();
    let mut lines: Vec<&str> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !lines.is_empty() {
                groups.push(group(&lines));
                lines.clear();
            }
        } else {
            lines.push(line);
        }
    }
    if !lines.is_empty() {
        groups.push(group(&lines));
    }
    groups
}

fn group(lines: &[&str]) -> Group {
    Group {
        webhook: lines[0].to_owned(),
        urls: lines[1..].iter().map(|url| (*url).to_owned()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_groups() {
        let text = "\
https://discord.example/one
https://olx.pt/search/one
https://olx.pt/search/two

https://discord.example/two
https://olx.pt/search/three
";
        assert_eq!(
            parse(text),
            vec![
                Group {
                    webhook: "https://discord.example/one".into(),
                    urls: vec![
                        "https://olx.pt/search/one".into(),
                        "https://olx.pt/search/two".into(),
                    ],
                },
                Group {
                    webhook: "https://discord.example/two".into(),
                    urls: vec!["https://olx.pt/search/three".into()],
                },
            ]
        );
    }

    #[test]
    fn trims_whitespace_and_ignores_blank_runs() {
        let text = "  https://discord.example/one  \n  https://olx.pt/search  \n\n\n";

        assert_eq!(
            parse(text),
            vec![Group {
                webhook: "https://discord.example/one".into(),
                urls: vec!["https://olx.pt/search".into()],
            }]
        );
    }

    #[test]
    fn empty_config_has_no_groups() {
        assert!(parse("").is_empty());
        assert!(parse("\n\n").is_empty());
    }
}
