/// Render CommonMark, then sanitize the resulting HTML. Raw executable HTML,
/// dangerous URL schemes and remote images are never trusted.
pub fn render(source: &str) -> String {
    let parser = pulldown_cmark::Parser::new_ext(
        source,
        pulldown_cmark::Options::ENABLE_STRIKETHROUGH | pulldown_cmark::Options::ENABLE_TABLES,
    );
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    sanitize(&html)
}
/// Only authenticated local attachment images may load inside Markdown.
/// Remote images remain excluded so reading a comment cannot contact a tracker.
pub(crate) fn sanitize(html: &str) -> String {
    let clean = ammonia::Builder::default()
        .attribute_filter(|tag, attribute, value| {
            if tag == "img" && attribute == "src" {
                let id = value
                    .strip_prefix("/api/v1/attachments/")
                    .and_then(|path| path.strip_suffix("/preview"));
                return id
                    .filter(|id| {
                        !id.is_empty()
                            && id
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                    })
                    .map(|_| std::borrow::Cow::Borrowed(value));
            }
            Some(std::borrow::Cow::Borrowed(value))
        })
        .clean(html)
        .to_string();
    // Ammonia removes a rejected src attribute. Remove the remaining empty image
    // element too, avoiding broken-image placeholders for excluded remote images.
    let mut result = String::new();
    let mut remainder = clean.as_str();
    while let Some(start) = remainder.find("<img") {
        result.push_str(&remainder[..start]);
        let Some(end) = remainder[start..].find('>') else {
            break;
        };
        let image = &remainder[start..start + end + 1];
        if image.contains(" src=\"") {
            result.push_str(image);
        }
        remainder = &remainder[start + end + 1..];
    }
    result.push_str(remainder);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permits_only_local_attachment_images() {
        let html = render(
            "![local](/api/v1/attachments/abc2345/preview) ![legacy](/api/v1/attachments/a_knkwp2ha22aw/preview) ![remote](https://tracker.invalid/a.png) ![path](/api/v1/attachments/../preview)",
        );
        assert_eq!(html.matches("<img ").count(), 2);
        assert!(html.contains("src=\"/api/v1/attachments/abc2345/preview\""));
        assert!(html.contains("src=\"/api/v1/attachments/a_knkwp2ha22aw/preview\""));
        assert!(!html.contains("tracker.invalid"));
    }
    #[test]
    fn renders_markdown_and_removes_executable_html_and_tracking_images() {
        let html = render(
            "**Brief**\n\n<script>alert(1)</script><img src='https://tracker.invalid/x' onerror='alert(1)'>\n\n[bad](javascript:alert(1))\n\n`<script>`",
        );
        assert!(html.contains("<strong>Brief</strong>"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("javascript:"));
        assert!(!html.contains("<img"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
