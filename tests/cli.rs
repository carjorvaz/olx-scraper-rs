//! End-to-end test: a local HTTP server serves a fixture page and captures the
//! webhook posts, so the whole binary runs without touching OLX or Discord.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;

const PAGE: &str = include_str!("fixtures/card.html");

struct Server {
    port: u16,
    posts: Arc<Mutex<Vec<String>>>,
}

fn start_server() -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let port = listener.local_addr().expect("server addr").port();
    let posts = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&posts);

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let captured = Arc::clone(&captured);
            thread::spawn(move || handle(stream, &captured));
        }
    });

    Server { port, posts }
}

fn handle(mut stream: TcpStream, posts: &Mutex<Vec<String>>) {
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
        let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
    } else {
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            PAGE.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(PAGE.as_bytes());
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

#[test]
fn scrapes_posts_and_dedupes_end_to_end() {
    let server = start_server();
    let dir = temp_dir("e2e");
    let config = dir.join("config");
    fs::write(
        &config,
        format!(
            "http://127.0.0.1:{}/webhook\nhttp://127.0.0.1:{}/search\n",
            server.port, server.port
        ),
    )
    .expect("write config");
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
