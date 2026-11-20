use linkify::{LinkFinder, LinkKind};
use pulldown_cmark::{CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};

pub(crate) const OPTIONS: Options = Options::ENABLE_STRIKETHROUGH.union(Options::ENABLE_TABLES);

/// Render CommonMark, then sanitize the resulting HTML. Raw executable HTML,
/// dangerous URL schemes and remote images are never trusted.
pub fn render(source: &str) -> String {
    render_events(Parser::new_ext(source, OPTIONS))
}

pub(crate) fn web_links<'a>(
    finder: &'a LinkFinder,
    text: &'a str,
) -> impl Iterator<Item = (usize, usize)> {
    finder.links(text).filter_map(|link| {
        let value = link.as_str();
        let web_url = match value.split_once("://") {
            Some((scheme, _)) => {
                scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")
            }
            None => value
                .get(..4)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("www.")),
        };
        web_url.then(|| (link.start(), link.end()))
    })
}

pub(crate) fn link_finder() -> LinkFinder {
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]).url_must_have_scheme(false);
    finder
}

pub(crate) fn link<'a>(url: CowStr<'a>, text: CowStr<'a>, title: CowStr<'a>) -> [Event<'a>; 3] {
    [
        Event::Start(Tag::Link {
            link_type: LinkType::Inline,
            dest_url: url,
            title,
            id: CowStr::Borrowed(""),
        }),
        Event::Text(text),
        Event::End(TagEnd::Link),
    ]
}

pub(crate) fn render_events<'a>(events: impl IntoIterator<Item = Event<'a>>) -> String {
    let finder = link_finder();
    let mut output = Vec::new();
    let mut text = String::new();
    let mut excluded = 0usize;
    let flush = |text: &mut String, output: &mut Vec<Event<'a>>| {
        let mut cursor = 0;
        for (start, end) in web_links(&finder, text) {
            if start > cursor {
                output.push(Event::Text(text[cursor..start].to_owned().into()));
            }
            let label = &text[start..end];
            let url = if label.contains("://") {
                label.to_owned()
            } else {
                format!("https://{label}")
            };
            output.extend(link(url.into(), label.to_owned().into(), "".into()));
            cursor = end;
        }
        if cursor < text.len() {
            output.push(Event::Text(text[cursor..].to_owned().into()));
        }
        text.clear();
    };
    for event in events {
        if let Event::Text(chunk) = &event
            && excluded == 0
        {
            text.push_str(chunk);
            continue;
        }
        flush(&mut text, &mut output);
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::Link { .. } | Tag::Image { .. }) => excluded += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::Link | TagEnd::Image) => {
                excluded = excluded.saturating_sub(1);
            }
            _ => {}
        }
        output.push(event);
    }
    flush(&mut text, &mut output);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, output.into_iter());
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
    fn links_pasted_web_urls_and_keeps_markdown_links_and_code_intact() {
        let html = render(
            "See (https://example.test/a_(b)?x=1&y=2), [label](https://example.test/explicit), \
             `https://example.test/code` and [https://example.test/label](/local). Visit www.example.test.",
        );
        assert_eq!(html.matches("<a ").count(), 4);
        assert!(html.contains("href=\"https://example.test/a_(b)?x=1&amp;y=2\""));
        assert!(html.contains(">label</a>"));
        assert!(html.contains("<code>https://example.test/code</code>"));
        assert!(html.contains(">https://example.test/label</a>"));
        assert!(html.contains("href=\"https://www.example.test\""));
    }
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
