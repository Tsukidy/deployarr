//! Finding 17, rewritten from its output on #82. sonnet/urljoin did not survive.
//!
//! An HTTP connection's base URL is `https://graph.microsoft.com`, and the host adds the
//! connection's credentials to every request it sends there. A guest supplies a `path`.
//! For each hostile path, the probe prints which host the request would go to when the host
//! builds the URL in one of two natural ways:
//!
//! - `Url::join`, from the `url` crate
//! - `format!("{base}{path}")`, then `Url::parse`
//!
//! The last line of each case uses the design's change. The guest's text is one path
//! segment, the host percent-encodes it with `path_segments_mut`, never parses it as a URL,
//! and checks that the host did not change.

use url::Url;

const BASE: &str = "https://graph.microsoft.com";

fn host(r: Result<Url, url::ParseError>) -> String {
    match r {
        Ok(u) => format!("{u}   (host {})", u.host_str().unwrap_or("none")),
        Err(e) => format!("error: {e}"),
    }
}

fn segments(base: &Url, list: &[&str]) -> String {
    if list.iter().any(|s| s.is_empty() || *s == "." || *s == "..") {
        return "refused: empty, `.`, or `..` segment".to_string();
    }
    let mut u = base.clone();
    u.path_segments_mut().expect("https URLs have a path").pop_if_empty().extend(list);
    if u.host_str() != base.host_str() {
        return format!("refused: host changed to {:?}", u.host_str());
    }
    host(Ok(u))
}

fn main() {
    let base = Url::parse(BASE).unwrap();
    println!("control: the segments [\"v1.0\", \"me\"]");
    println!("    encoded segments       -> {}", segments(&base, &["v1.0", "me"]));
    let paths = [
        "https://attacker.example/x",
        "//attacker.example/x",
        "@attacker.example/x",
        "/../../x",
        "\\\\attacker.example\\x",
        "..",
    ];
    for path in paths {
        println!("guest path {path:?}");
        println!("    Url::join              -> {}", host(base.join(path)));
        println!("    format!(\"{{base}}{{path}}\") -> {}", host(Url::parse(&format!("{BASE}{path}"))));
        println!("    one encoded segment    -> {}", segments(&base, &[path]));
    }
}
