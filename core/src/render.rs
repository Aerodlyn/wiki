use crate::page::Page;
use comrak::nodes::{AstNode, NodeValue};
use comrak::{format_html, parse_document, Arena, Options};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub target: String,
    pub shown: Option<String>,
    pub embed: bool, // true for ![[...]]
}

const IMAGE_NAME: &AsciiSet = &NON_ALPHANUMERIC.remove(b'.').remove(b'-').remove(b'_');

static SANITIZER: LazyLock<ammonia::Builder<'static>> = LazyLock::new(|| {
    let mut b = ammonia::Builder::default();
    b.add_tag_attributes("a", &["class", "data-title"])
        .add_url_schemes(&["wiki-image"])
        .link_rel(None);
    b
});

/// Extract all links from the given markdown text.
pub fn extract_links(text: &str) -> Vec<Link> {
    let arena = Arena::new();
    let root = parse_document(&arena, text, &options());
    links_in(root)
        .into_iter()
        .map(|(_, l)| l)
        .filter(|l| !l.target.is_empty())
        .collect()
}

/// Extract all links from a page, including those in the infobox.
pub fn page_links(page: &Page) -> Vec<Link> {
    let mut links = extract_links(&page.body);
    if let Some(ib) = &page.front.infobox {
        for f in &ib.fields {
            links.extend(extract_links(&f.value));
        }
    }
    links
}

/// Markdown → sanitized HTML. `exists` gets each link target as typed and says whether that page exists.
pub fn render(markdown: &str, exists: impl Fn(&str) -> bool) -> String {
    let arena = Arena::new();
    let root = parse_document(&arena, markdown, &options());

    for (node, link) in links_in(root) {
        let html = arena.alloc(NodeValue::HtmlInline(link_html(&link, &exists)).into());
        node.insert_before(html);
        node.detach();
    }

    let mut out = String::new();
    format_html(root, &options(), &mut out).unwrap();
    SANITIZER.clean(&out).to_string()
}

pub fn render_inline(text: &str, exists: impl Fn(&str) -> bool) -> String {
    let html = render(text, &exists);
    match html.strip_prefix("<p>").and_then(|s| s.strip_suffix("</p>\n")) {
        Some(inner) if !inner.contains("<p>") => inner.to_string(),
        _ => html,
    }
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    comrak::html::escape(&mut out, s).unwrap();
    out
}

fn link_html(link: &Link, exists: impl Fn(&str) -> bool) -> String {
    if link.target.is_empty() {
        let bang = if link.embed { "!" } else { "" };
        let pipe = link
            .shown
            .as_ref()
            .map(|s| format!("|{s}"))
            .unwrap_or_default();
        return escape(&format!("{bang}[[{pipe}]]"));
    }

    if link.embed {
        let src = utf8_percent_encode(&link.target, IMAGE_NAME);
        return format!(
            r##"<img src="wiki-image://localhost/{src}" alt="{}">"##,
            escape(&link.target)
        );
    }

    let class = if exists(&link.target) {
        "wikilink"
    } else {
        "wikilink missing"
    };
    let shown = link.shown.as_deref().unwrap_or(&link.target);

    format!(
        r##"<a class="{class}" data-title="{}" href="#">{}</a>"##,
        escape(&link.target),
        escape(shown)
    )
}

fn links_in<'a>(root: &'a AstNode<'a>) -> Vec<(&'a AstNode<'a>, Link)> {
    root.descendants()
        .filter_map(|n| {
            let NodeValue::WikiLink(w) = &n.data().value else {
                return None;
            };
            let target = collapse(&w.url);
            let label = collapse(&text_of(n));
            let shown = (!label.is_empty() && label != target).then_some(label);
            Some((
                n,
                Link {
                    target,
                    shown,
                    embed: w.embed,
                },
            ))
        })
        .collect()
}

fn options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.wikilinks_title_after_pipe = true;
    opts.extension.table = true;
    opts.extension.strikethrough = true;
    opts.render.r#unsafe = true;
    opts
}

fn text_of<'a>(node: &'a AstNode<'a>) -> String {
    node.descendants()
        .filter_map(|n| match &n.data().value {
            NodeValue::Text(t) => Some(t.to_string()),
            NodeValue::Code(c) => Some(c.literal.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::parse_page;

    fn link(target: &str, shown: Option<&str>, embed: bool) -> Link {
        Link {
            target: target.into(),
            shown: shown.map(Into::into),
            embed,
        }
    }

    fn known(t: &str) -> bool {
        ["orhold", "queen ilsa"].contains(&t.to_lowercase().as_str())
    }

    #[test]
    fn finds_links_and_embeds() {
        let text = "Ruled from [[Orhold]] by [[Queen Ilsa|the queen]].\n\n![[varn-banner.jpg]]\n";

        assert_eq!(
            extract_links(text),
            vec![
                link("Orhold", None, false),
                link("Queen Ilsa", Some("the queen"), false),
                link("varn-banner.jpg", None, true),
            ]
        );
    }

    #[test]
    fn target_trimmed_case_kept() {
        assert_eq!(
            extract_links("[[ orHold ]]"),
            vec![link("orHold", None, false)]
        );
    }

    #[test]
    fn soft_wrapped_link_is_one_title() {
        assert_eq!(
            extract_links("by [[Queen\nIlsa]] and [[Queen  Ilsa|the\nqueen]]"),
            vec![
                link("Queen Ilsa", None, false),
                link("Queen Ilsa", Some("the queen"), false),
            ]
        );
    }

    #[test]
    fn code_is_not_links() {
        assert_eq!(
            extract_links("`[[A]]`\n\n```\n[[B]]\n```\n\n    [[C]]\n"),
            vec![]
        );
    }

    #[test]
    fn empty_links_are_ignored() {
        assert_eq!(extract_links("[[]] [[|x]] ![[]]"), vec![]);
    }

    #[test]
    fn page_links_include_infobox_values() {
        let p = parse_page(
            "T",
            "",
            "---\ninfobox:\n  fields:\n    - Capital: \"[[Orhold]]\"\n---\n\nSee [[Saltmere]].\n",
        );
        let targets: Vec<_> = page_links(&p).into_iter().map(|l| l.target).collect();

        assert_eq!(targets, vec!["Saltmere", "Orhold"]);
    }

    #[test]
    fn blue_and_red_links() {
        let html = render("[[Orhold]] and [[Ash Covenant|the Covenant]]", known);

        assert_eq!(
            html,
            "<p><a class=\"wikilink\" data-title=\"Orhold\" href=\"#\">Orhold</a> and \
            <a class=\"wikilink missing\" data-title=\"Ash Covenant\" href=\"#\">the Covenant</a></p>\n"
        );
    }

    #[test]
    fn exists_gets_target_as_typed() {
        let html = render("[[orhold]]", |t| t == "orhold");

        assert!(html.contains("class=\"wikilink\""), "{html}");
        assert!(html.contains("data-title=\"orhold\""), "{html}");
    }

    #[test]
    fn image_embed() {
        assert_eq!(
            render("![[varn banner.jpg]]", known),
            "<p><img src=\"wiki-image://localhost/varn%20banner.jpg\" alt=\"varn banner.jpg\"></p>\n"
        );
    }

    #[test]
    fn titles_are_escaped() {
        let html = render("[[Tom & \"Jerry\" <Inn>]]", |_| true);

        assert!(
            html.contains("data-title=\"Tom &amp; &quot;Jerry&quot; &lt;Inn&gt;\""),
            "{html}"
        );
        assert!(
            html.contains(">Tom &amp; \"Jerry\" &lt;Inn&gt;</a>"),
            "{html}"
        );
    }

    #[test]
    fn empty_links_render_as_text() {
        let html = render("[[]] [[|x]]", known);

        assert!(!html.contains("<a"), "{html}");
        assert!(html.contains("[[|x]]"), "{html}");
    }

    #[test]
    fn dangerous_html_is_stripped() {
        let html = render(
            "<script>alert(1)</script>\n\n<b onclick=\"x()\">bold</b> [x](javascript:alert(1)) <img src=\"https://e.com/x.png\">",
            known
        );

        assert!(!html.contains("script"), "{html}");
        assert!(!html.contains("onclick"), "{html}");
        assert!(!html.contains("javascript"), "{html}");
        assert!(html.contains("<b>bold</b>"), "{html}");
    }

    #[test]
    fn markdown_basics_survive() {
        let html = render(
            "## History\n\nSome *em* and ~~gone~~.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
            known,
        );

        for s in [
            "<h2>History</h2>",
            "<em>em</em>",
            "<del>gone</del>",
            "<table>",
            "<td>1</td>",
        ] {
            assert!(html.contains(s), "missing {s} in {html}");
        }
    }

    #[test]
    fn inline_values_have_no_paragraph() {
        assert_eq!(
            render_inline("[[Orhold]], founded 412 AR", known),
            "<a class=\"wikilink\" data-title=\"Orhold\" href=\"#\">Orhold</a>, founded 412 AR"
        );
        assert_eq!(render_inline("", known), "");
        assert_eq!(render_inline("a\n\nb", known), "<p>a</p>\n<p>b</p>\n");
    }
}
