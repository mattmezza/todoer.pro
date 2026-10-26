/// Render CommonMark, then sanitize the resulting HTML. Raw executable HTML,
/// dangerous URL schemes and remote images are never trusted.
pub fn render(source: &str) -> String {
    let parser = pulldown_cmark::Parser::new_ext(
        source,
        pulldown_cmark::Options::ENABLE_STRIKETHROUGH | pulldown_cmark::Options::ENABLE_TABLES,
    );
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    ammonia::Builder::default()
        .rm_tags(&["img"])
        .clean(&html)
        .to_string()
}
#[cfg(test)]
mod tests {
    use super::*;
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
