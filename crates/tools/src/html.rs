//! Minimal HTML → text. Scripts, styles and other non-content blocks are
//! dropped; no remote resources are ever loaded.

pub fn title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let open_end = lower[start..].find('>')? + start + 1;
    let end = lower[open_end..].find("</title>")? + open_end;
    let t = decode(html[open_end..end].trim());
    (!t.is_empty()).then_some(t)
}

pub fn to_text(html: &str) -> String {
    let mut text = html.to_owned();
    for tag in ["script", "style", "noscript", "svg", "template", "head"] {
        text = remove_blocks(&text, tag);
    }
    let mut out = String::with_capacity(text.len() / 2);
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in text.chars() {
        match (in_tag, ch) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let name = tag.trim_start_matches('/').split_whitespace().next().unwrap_or("").to_lowercase();
                if matches!(name.as_str(), "p" | "br" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "section" | "article") {
                    out.push('\n');
                }
            }
            (true, c) => tag.push(c),
            (false, c) => out.push(c),
        }
    }
    let decoded = decode(&out);
    let mut lines: Vec<String> = Vec::new();
    for line in decoded.lines() {
        let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
            lines.push(collapsed);
        }
    }
    lines.join("\n")
}

fn remove_blocks(html: &str, tag: &str) -> String {
    let lower = html.to_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    while let Some(start) = lower[pos..].find(&open).map(|i| i + pos) {
        out.push_str(&html[pos..start]);
        match lower[start..].find(&close) {
            Some(end) => pos = start + end + close.len(),
            None => {
                pos = html.len();
                break;
            }
        }
    }
    out.push_str(&html[pos.min(html.len())..]);
    out
}

fn decode(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_and_styles_are_dropped() {
        let html = "<html><head><title>Hoi &amp; dag</title><style>p{}</style></head><body><script>alert(1)</script><h1>Kop</h1><p>Een <b>tekst</b>.</p></body></html>";
        assert_eq!(title(html).as_deref(), Some("Hoi & dag"));
        let text = to_text(html);
        assert!(text.contains("Kop"));
        assert!(text.contains("Een tekst."));
        assert!(!text.contains("alert"));
    }
}
