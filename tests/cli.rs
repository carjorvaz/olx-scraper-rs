//! End-to-end tests: a local HTTP server serves a fixture page and captures the
//! webhook posts, so the whole binary runs without touching OLX or Discord.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

const PAGE: &str = include_str!("fixtures/card.html");

/// A page that claims to have a result but has no card to extract.
const DRIFT_PAGE: &str =
    r#"<!doctype html><html><body><p data-testid="total-count">1 anúncio</p></body></html>"#;

struct Server {
    port: u16,
    posts: Arc<Mutex<Vec<String>>>,
    attempts: Arc<AtomicUsize>,
}

fn start_server() -> Server {
    start_server_with(PAGE, 204, false)
}

fn start_server_with(page: &'static str, post_status: u16, rate_limit_first: bool) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let port = listener.local_addr().expect("server addr").port();
    let posts = Arc::new(Mutex::new(Vec::new()));
    let attempts = Arc::new(AtomicUsize::new(0));
    let captured_posts = Arc::clone(&posts);
    let captured_attempts = Arc::clone(&attempts);

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let posts = Arc::clone(&captured_posts);
            let attempts = Arc::clone(&captured_attempts);
            thread::spawn(move || {
                handle(
                    stream,
                    &posts,
                    &attempts,
                    page,
                    post_status,
                    rate_limit_first,
                );
            });
        }
    });

    Server {
        port,
        posts,
        attempts,
    }
}

fn handle(
    mut stream: TcpStream,
    posts: &Mutex<Vec<String>>,
    attempts: &AtomicUsize,
    page: &str,
    post_status: u16,
    rate_limit_first: bool,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request = String::new();
    reader.read_line(&mut request).expect("request line");

    let mut content_length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).expect("header") == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().expect("content length");
        }
    }

    if request.starts_with("POST ") {
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body).expect("request body");
        posts
            .lock()
            .expect("posts lock")
            .push(String::from_utf8(body).expect("utf-8 body"));

        let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
        let (status, extra) = if rate_limit_first && attempt == 1 {
            (429, "Retry-After: 0\r\n")
        } else {
            (post_status, "")
        };
        let reason = match status {
            204 => "No Content",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            _ => "Status",
        };
        let response = format!("HTTP/1.1 {status} {reason}\r\n{extra}Connection: close\r\n\r\n");
        let _ = stream.write_all(response.as_bytes());
    } else {
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            page.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(page.as_bytes());
    }
}

fn run(config: &Path, state: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_olx-scraper"))
        .env("CONFIG_FILE", config)
        .env("STATE_DIR", state)
        .output()
        .expect("run olx-scraper")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("olx-scraper-cli-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_config(dir: &Path, port: u16) -> PathBuf {
    let config = dir.join("config");
    fs::write(
        &config,
        format!("http://127.0.0.1:{port}/webhook\nhttp://127.0.0.1:{port}/search\n"),
    )
    .expect("write config");
    config
}

#[test]
fn scrapes_posts_and_dedupes_end_to_end() {
    let server = start_server();
    let dir = temp_dir("e2e");
    let config = write_config(&dir, server.port);
    let state = dir.join("state");

    let first = run(&config, &state);
    let stdout = String::from_utf8_lossy(&first.stdout);
    assert!(
        first.status.success(),
        "first run failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(stdout.contains("Found 1 listings, 1 new"), "{stdout}");

    let posts = server.posts.lock().expect("posts lock").clone();
    assert_eq!(posts.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(&posts[0]).expect("json body");
    assert_eq!(payload["embeds"][0]["title"], "Hyndai Ioniq eletrico 28kw");
    assert_eq!(
        payload["embeds"][0]["url"],
        "https://olx.pt/d/anuncio/hyndai-ioniq-eletrico-28kw-IDJBBvC.html"
    );
    assert_eq!(payload["embeds"][0]["fields"][0]["name"], "Preço");

    let seen = fs::read_to_string(state.join("seen-urls.txt")).expect("state file");
    assert!(seen.contains("https://olx.pt/d/anuncio/hyndai-ioniq-eletrico-28kw-IDJBBvC.html"));

    let second = run(&config, &state);
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(
        second.status.success(),
        "second run failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(stdout.contains("Sent 0 new listings total."), "{stdout}");
    assert_eq!(server.posts.lock().expect("posts lock").len(), 1);

    fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn a_failed_post_exits_nonzero_and_leaves_the_listing_unseen() {
    let server = start_server_with(PAGE, 500, false);
    let dir = temp_dir("failed-post");
    let config = write_config(&dir, server.port);
    let state = dir.join("state");

    let output = run(&config, &state);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("HTTP 500"), "{stderr}");
    assert_eq!(server.posts.lock().expect("posts lock").len(), 1);

    let seen = fs::read_to_string(state.join("seen-urls.txt")).expect("state file");
    assert!(!seen.contains("hyndai-ioniq-eletrico-28kw"), "{seen}");

    fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn a_failed_group_does_not_stop_the_next_group() {
    let failing = start_server_with(PAGE, 500, false);
    let working = start_server();
    let dir = temp_dir("group-isolation");
    let config = dir.join("config");
    fs::write(
        &config,
        format!(
            "http://127.0.0.1:{fail}/webhook\nhttp://127.0.0.1:{fail}/search\n\nhttp://127.0.0.1:{ok}/webhook\nhttp://127.0.0.1:{ok}/search\n",
            fail = failing.port,
            ok = working.port
        ),
    )
    .expect("write config");
    let state = dir.join("state");

    let output = run(&config, &state);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("HTTP 500"), "{stderr}");
    assert_eq!(failing.attempts.load(Ordering::SeqCst), 1);
    assert_eq!(working.attempts.load(Ordering::SeqCst), 1);

    let seen = fs::read_to_string(state.join("seen-urls.txt")).expect("state file");
    assert!(seen.contains("hyndai-ioniq-eletrico-28kw"), "{seen}");

    fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn all_failed_searches_exit_nonzero() {
    let dir = temp_dir("all-failed");
    let config = dir.join("config");
    fs::write(
        &config,
        "http://127.0.0.1:1/webhook\nhttp://127.0.0.1:1/search\n",
    )
    .expect("write config");
    let state = dir.join("state");

    let output = run(&config, &state);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("all searches failed"), "{stderr}");

    fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn drifted_pages_exit_nonzero() {
    let server = start_server_with(DRIFT_PAGE, 204, false);
    let dir = temp_dir("drift");
    let config = write_config(&dir, server.port);
    let state = dir.join("state");

    let output = run(&config, &state);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no cards"), "{stderr}");
    assert_eq!(server.attempts.load(Ordering::SeqCst), 0);

    fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn retries_a_rate_limited_post() {
    let server = start_server_with(PAGE, 204, true);
    let dir = temp_dir("rate-limit");
    let config = write_config(&dir, server.port);
    let state = dir.join("state");

    let output = run(&config, &state);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Sent 1 new listings total."), "{stdout}");
    assert_eq!(server.attempts.load(Ordering::SeqCst), 2);
    assert_eq!(server.posts.lock().expect("posts lock").len(), 2);

    let seen = fs::read_to_string(state.join("seen-urls.txt")).expect("state file");
    assert!(seen.contains("hyndai-ioniq-eletrico-28kw"), "{seen}");

    fs::remove_dir_all(dir).expect("remove temp dir");
}
