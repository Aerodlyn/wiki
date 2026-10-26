use crate::page::Page;
use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub target: String,
    pub shown: Option<String>,
    pub embed: bool, // true for ![[...]]
}

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

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.wikilinks_title_after_pipe = true;
    opts.extension.table = true;
    opts.extension.strikethrough = true;
    opts.render.r#unsafe = true;
    opts
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
}
