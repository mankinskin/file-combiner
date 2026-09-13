use dioxus::prelude::*;
use pulldown_cmark::{html, CowStr, Event, Options, Parser};

pub(crate) fn render_markdown(source: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(source, options).map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => {
            Event::Text(CowStr::Boxed(raw.into_string().into_boxed_str()))
        }
        other => other,
    });
    let mut rendered = String::new();
    html::push_html(&mut rendered, parser);
    rendered
}

#[component]
pub(crate) fn MarkdownView(content: String, label: String) -> Element {
    let rendered = render_markdown(&content);

    rsx! {
        div {
            class: "markdown-content",
            "aria-label": "{label}",
            dangerous_inner_html: "{rendered}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::render_markdown;

    #[test]
    fn renders_common_markdown_structure() {
        let rendered = render_markdown("# Title\n\n- one\n- two\n\n**strong**");

        assert!(rendered.contains("<h1>Title</h1>"));
        assert!(rendered.contains("<li>one</li>"));
        assert!(rendered.contains("<strong>strong</strong>"));
    }

    #[test]
    fn escapes_raw_html_in_markdown_source() {
        let rendered = render_markdown("<script>alert('x')</script>");

        assert!(rendered.contains("&lt;script&gt;"));
        assert!(!rendered.contains("<script>"));
    }
}
