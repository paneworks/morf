//! What a clipboard or a drop is offering, and which of it to take.
//!
//! Pure functions, no protocol: the same questions are asked of a data-control
//! selection, a `wl_data_device` selection and a drag, and the answers must be
//! the same for all three. A source lists every type it can produce, in its
//! own order and under whatever names its toolkit prefers — `UTF8_STRING`
//! from X11 heritage, `text/plain;charset=utf-8` from GTK, `text/uri-list`
//! from a file manager — and a shell asking for "the text" should not have to
//! know which it got.

/// Text types, best first. `charset=utf-8` is the one that promises UTF-8;
/// plain `text/plain` very nearly always is; the X11 names are what XWayland
/// and older toolkits put on the clipboard.
pub const TEXT_MIMES: [&str; 6] = [
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];

/// Image types, best first: lossless and universally decodable ahead of the
/// rest, so a thumbnail is not a JPEG of a PNG.
pub const IMAGE_MIMES: [&str; 6] = [
    "image/png",
    "image/webp",
    "image/jpeg",
    "image/gif",
    "image/bmp",
    "image/svg+xml",
];

/// A list of URIs, one per line, as file managers drag and copy files.
pub const URI_LIST_MIME: &str = "text/uri-list";

/// The most a single read may return, in bytes.
///
/// Large enough for a full-resolution screenshot on the clipboard, small
/// enough that a runaway source cannot take the shell's memory with it.
pub const MAX_OFFER_BYTES: usize = 32 * 1024 * 1024;

/// The best text type among `offered`, if there is one.
pub fn best_text_mime(offered: &[String]) -> Option<&str> {
    TEXT_MIMES
        .iter()
        .find_map(|preferred| offered.iter().find(|mime| mime == preferred))
        .or_else(|| {
            // Anything else under `text/` that is not a URI list — `text/html`
            // is text, just not the text anyone means by "the text".
            offered.iter().find(|mime| mime.starts_with("text/plain"))
        })
        .map(String::as_str)
}

/// The best image type among `offered`, if there is one.
pub fn best_image_mime(offered: &[String]) -> Option<&str> {
    IMAGE_MIMES
        .iter()
        .find_map(|preferred| offered.iter().find(|mime| mime == preferred))
        .or_else(|| offered.iter().find(|mime| mime.starts_with("image/")))
        .map(String::as_str)
}

/// Turns what a configuration asked to read into a type the source offers.
///
/// `"text"` and `"image"` are shorthands for the best of each; `"uris"` and
/// `"files"` name the URI list. Anything else must be offered exactly — a
/// source that did not list a type cannot be asked for it.
pub fn resolve_mime(requested: &str, offered: &[String]) -> Option<String> {
    match requested {
        "text" => best_text_mime(offered).map(str::to_owned),
        "image" => best_image_mime(offered).map(str::to_owned),
        "uris" | "files" => offered.iter().find(|mime| *mime == URI_LIST_MIME).cloned(),
        exact => offered.iter().find(|mime| *mime == exact).cloned(),
    }
}

/// Whether one accepted key covers one offered type.
///
/// A key is an exact type, a `major/*` wildcard, `*` for anything, or one of
/// the shorthands `text`, `image`, `uris`/`files`.
pub fn key_matches(key: &str, mime: &str) -> bool {
    match key {
        "*" | "*/*" => true,
        "text" => TEXT_MIMES.contains(&mime) || mime.starts_with("text/plain"),
        "image" => mime.starts_with("image/"),
        "uris" | "files" => mime == URI_LIST_MIME,
        key => match key.strip_suffix("/*") {
            Some(major) => mime
                .split_once('/')
                .is_some_and(|(offered, _)| offered == major),
            None => key == mime,
        },
    }
}

/// Which offered type a drop target should accept, if any.
///
/// The keys are the target's preferences in order, so the first key that any
/// offered type satisfies decides; within one key the source's own order
/// breaks ties, since it listed its best first. No keys means the target takes
/// anything, preferring files, then text, then whatever came first.
pub fn accept_mime(keys: &[String], offered: &[String]) -> Option<String> {
    if keys.is_empty() {
        return offered
            .iter()
            .find(|mime| *mime == URI_LIST_MIME)
            .map(String::as_str)
            .or_else(|| best_text_mime(offered))
            .or_else(|| offered.first().map(String::as_str))
            .map(str::to_owned);
    }
    keys.iter().find_map(|key| match key.as_str() {
        // The shorthands pick the best of their kind, not merely the first.
        "text" => best_text_mime(offered).map(str::to_owned),
        "image" => best_image_mime(offered).map(str::to_owned),
        key => offered.iter().find(|mime| key_matches(key, mime)).cloned(),
    })
}

/// Parses `text/uri-list` into its URIs.
///
/// RFC 2483: one URI per line, CRLF separated — though LF alone is common and
/// accepted — with lines starting `#` as comments. Blank lines are dropped.
pub fn parse_uri_list(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// The local path a `file://` URI names, percent-decoded.
///
/// Only local files: an empty host or `localhost`. A URI for another host, or
/// not a file at all, has no path here.
pub fn uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let path = if let Some(path) = rest.strip_prefix("localhost") {
        path
    } else {
        rest
    };
    if !path.starts_with('/') {
        return None;
    }
    let mut bytes = Vec::with_capacity(path.len());
    let raw = path.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%'
            && index + 2 < raw.len()
            && let (Some(high), Some(low)) = (hex(raw[index + 1]), hex(raw[index + 2]))
        {
            bytes.push(high << 4 | low);
            index += 3;
            continue;
        }
        bytes.push(raw[index]);
        index += 1;
    }
    String::from_utf8(bytes).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// A `file://` URI for a local path, percent-encoding what a URI may not hold.
pub fn path_to_uri(path: &str) -> String {
    let mut uri = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~!$&'()*+,;=:@".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// Joins URIs into a `text/uri-list` body.
pub fn encode_uri_list<S: AsRef<str>>(uris: &[S]) -> String {
    let mut body = String::new();
    for uri in uris {
        body.push_str(uri.as_ref());
        body.push_str("\r\n");
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn text_prefers_declared_utf8() {
        let offered = list(&["TEXT", "text/plain", "text/plain;charset=utf-8", "STRING"]);
        assert_eq!(best_text_mime(&offered), Some("text/plain;charset=utf-8"));
        assert_eq!(best_text_mime(&list(&["UTF8_STRING"])), Some("UTF8_STRING"));
        assert_eq!(best_text_mime(&list(&["text/html", "image/png"])), None);
        assert_eq!(
            best_text_mime(&list(&["text/plain;charset=iso-8859-1"])),
            Some("text/plain;charset=iso-8859-1")
        );
    }

    #[test]
    fn image_prefers_png_then_any_image() {
        let offered = list(&["image/jpeg", "image/png", "text/plain"]);
        assert_eq!(best_image_mime(&offered), Some("image/png"));
        assert_eq!(
            best_image_mime(&list(&["image/x-foo"])),
            Some("image/x-foo")
        );
        assert_eq!(best_image_mime(&list(&["text/plain"])), None);
    }

    #[test]
    fn resolve_expands_shorthands_and_requires_exact_otherwise() {
        let offered = list(&["text/uri-list", "UTF8_STRING", "image/png"]);
        assert_eq!(
            resolve_mime("text", &offered).as_deref(),
            Some("UTF8_STRING")
        );
        assert_eq!(
            resolve_mime("image", &offered).as_deref(),
            Some("image/png")
        );
        assert_eq!(
            resolve_mime("files", &offered).as_deref(),
            Some("text/uri-list")
        );
        assert_eq!(
            resolve_mime("image/png", &offered).as_deref(),
            Some("image/png")
        );
        assert_eq!(resolve_mime("image/jpeg", &offered), None);
    }

    #[test]
    fn keys_match_wildcards_and_shorthands() {
        assert!(key_matches("*", "anything/at-all"));
        assert!(key_matches("image/*", "image/png"));
        assert!(!key_matches("image/*", "text/plain"));
        assert!(key_matches("text", "UTF8_STRING"));
        assert!(key_matches("files", "text/uri-list"));
        assert!(!key_matches("text", "text/uri-list"));
        assert!(key_matches("text/html", "text/html"));
    }

    #[test]
    fn accept_follows_key_order_and_defaults() {
        let offered = list(&["text/plain", "text/uri-list", "image/png"]);
        assert_eq!(
            accept_mime(&list(&["image", "files"]), &offered).as_deref(),
            Some("image/png")
        );
        assert_eq!(
            accept_mime(&list(&["application/pdf", "text"]), &offered).as_deref(),
            Some("text/plain")
        );
        assert_eq!(accept_mime(&list(&["application/pdf"]), &offered), None);
        assert_eq!(accept_mime(&[], &offered).as_deref(), Some("text/uri-list"));
        assert_eq!(
            accept_mime(&[], &list(&["application/x-thing"])).as_deref(),
            Some("application/x-thing")
        );
        assert_eq!(accept_mime(&[], &[]), None);
    }

    #[test]
    fn uri_lists_skip_comments_and_blank_lines() {
        let body = b"# dragged from a file manager\r\nfile:///home/a/b.txt\r\n\r\nhttps://example.org/x\nfile:///tmp/c%20d.png\r\n";
        assert_eq!(
            parse_uri_list(body),
            [
                "file:///home/a/b.txt",
                "https://example.org/x",
                "file:///tmp/c%20d.png"
            ]
        );
        assert!(parse_uri_list(b"").is_empty());
    }

    #[test]
    fn file_uris_become_paths_and_back() {
        assert_eq!(
            uri_to_path("file:///tmp/c%20d.png").as_deref(),
            Some("/tmp/c d.png")
        );
        assert_eq!(
            uri_to_path("file://localhost/etc/hosts").as_deref(),
            Some("/etc/hosts")
        );
        assert_eq!(uri_to_path("file://otherhost/etc/hosts"), None);
        assert_eq!(uri_to_path("https://example.org/x"), None);
        // A stray percent is kept as it is rather than failing the path.
        assert_eq!(uri_to_path("file:///a%2").as_deref(), Some("/a%2"));
        assert_eq!(path_to_uri("/tmp/c d.png"), "file:///tmp/c%20d.png");
        let round = path_to_uri("/home/ü/#1.txt");
        assert_eq!(uri_to_path(&round).as_deref(), Some("/home/ü/#1.txt"));
        assert_eq!(
            encode_uri_list(&["file:///a", "file:///b"]),
            "file:///a\r\nfile:///b\r\n"
        );
    }
}
