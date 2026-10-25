// use serde::de::Error as _;
use serde::{Deserialize, Serialize};
// use serde_yaml_ng::{Mapping, Value};
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub title: String,
    pub folder: String, // vault-relative, "" = vault root
    pub front: FrontMatter,
    pub front_error: Option<String>, // Some = front matter unreadable, body holds the whole file
    pub body: String,
    pub version: u64, // version_of(original text)
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrontMatter {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,

    #[serde(default, skip_serializing_if = "is_false")]
    pub quicknotes: bool,
}

impl FrontMatter {
    fn is_empty(&self) -> bool {
        *self == FrontMatter::default()
    }
}

/// Compute the version hash of the given text.
pub fn version_of(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Parse a page from the given title, folder, and text.
pub fn parse_page(title: &str, folder: &str, text: &str) -> Page {
    let mut page = Page {
        title: title.into(),
        folder: folder.into(),
        front: FrontMatter::default(),
        front_error: None,
        body: text.into(),
        version: version_of(text),
    };

    if let Some((yaml, body)) = split(text) {
        match serde_yaml_ng::from_str(yaml) {
            Ok(front) => {
                page.front = front;
                page.body = body.into();
            }
            Err(e) => page.front_error = Some(e.to_string()),
        }
    }

    page
}

/// Serialize a page into a string, including front matter if present.
pub fn serialize_page(page: &Page) -> String {
    if page.front.is_empty() {
        return page.body.clone();
    }

    let yaml = serde_yaml_ng::to_string(&page.front).expect("front matter serialization failed");
    format!("---\n{yaml}---\n\n{}", page.body)
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Split the given text into front matter and body, if front matter is present.
fn split(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let (yaml, body) = if let Some(b) = rest.strip_prefix("---\n") {
        ("", b)
    } else {
        match rest.find("\n---\n") {
            Some(i) => (&rest[..=i], &rest[i + 5..]),
            None => (rest.strip_suffix("\n---")?, ""),
        }
    };

    Some((yaml, body.strip_prefix("\n").unwrap_or(body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reparse(p: &Page) -> Page {
        parse_page(&p.title, &p.folder, &serialize_page(p))
    }

    #[test]
    fn no_front_matter() {
        let text = "Just text.\n\n## Heading";
        let p = parse_page("T", "", text);

        assert_eq!(p.front, FrontMatter::default());
        assert_eq!(p.front_error, None);
        assert_eq!(p.body, text);
        assert_eq!(serialize_page(&p), text);
    }

    #[test]
    fn version_tracks_content() {
        let a = parse_page("T", "", "x\n");

        assert_eq!(a.version, parse_page("T", "", "x\n").version);
        assert_ne!(a.version, parse_page("T", "", "y\n").version);
        assert_eq!(a.version, version_of("x\n"));
    }

    #[test]
    fn parses_categories_and_quicknotes() {
        let p = parse_page(
            "Varn",
            "Nations",
            "---\ncategories: [Nations, Northern Realms]\nquicknotes: true\n---\n\nVarn is cold.",
        );

        assert_eq!(p.front_error, None);
        assert_eq!(p.front.categories, vec!["Nations", "Northern Realms"]);
        assert!(p.front.quicknotes);
        assert_eq!(p.body, "Varn is cold.");
        assert_eq!((p.title.as_str(), p.folder.as_str()), ("Varn", "Nations"))
    }

    #[test]
    fn adding_a_category_writes_front_matter() {
        let mut p = parse_page("T", "", "x\n");
        p.front.categories.push("Test".into());

        assert_eq!(serialize_page(&p), "---\ncategories:\n- Test\n---\n\nx\n");
    }

    #[test]
    fn clearing_quicknote_removes_the_key() {
        let mut p = parse_page(
            "T",
            "",
            "---\nquicknotes: true\ncategories: [Test]\n---\n\nx\n",
        );
        p.front.quicknotes = false;

        assert!(!serialize_page(&p).contains("quicknotes"));
    }

    #[test]
    fn empty_front_matter_is_dropped_on_save() {
        let p = parse_page("T", "", "---\n---\nx\n");

        assert_eq!(p.front_error, None);
        assert_eq!(p.body, "x\n");
        assert_eq!(serialize_page(&p), "x\n");
    }

    #[test]
    fn closing_fence_at_end_of_file() {
        let p = parse_page("T", "", "---\ncategories: [Test]\n---");

        assert_eq!(p.front.categories, vec!["Test"]);
        assert_eq!(p.body, "");
    }

    #[test]
    fn leading_rule_without_closing_fence_is_body() {
        let text = "---\n\nA page that opens with a rule.\n";
        let p = parse_page("T", "", text);

        assert_eq!(p.front_error, None);
        assert_eq!(p.body, text);
    }

    #[test]
    fn horizontal_rule_in_body_survives() {
        let p = parse_page(
            "T",
            "",
            "---\ncategories: [Test]\n---\n\nAbove\n\n---\n\nBelow\n",
        );

        assert_eq!(p.front.categories, vec!["Test"]);
        assert_eq!(p.body, "Above\n\n---\n\nBelow\n");
        assert_eq!(reparse(&p).body, p.body);
    }

    #[test]
    fn invalid_yaml_round_trips_byte_for_byte() {
        let text = "---\ninfobox: [unclosed\n---\n\nx\n";
        let p = parse_page("T", "", text);

        assert!(p.front_error.is_some());
        assert_eq!(p.body, text);
        assert_eq!(serialize_page(&p), text);
    }

    #[test]
    fn wrong_shapes_fall_back_to_raw() {
        for text in [
            "---\n- just\n- a list\n---\nx\n",
            "---\ncategories: Nations\n---\nx\n",
        ] {
            let p = parse_page("T", "", text);

            assert!(p.front_error.is_some(), "{text}");
            assert_eq!(serialize_page(&p), text);
        }
    }

    #[test]
    fn duplicate_keys_fall_back_to_raw() {
        let text = "---\ncategories: [Test]\ncategories: [Duplicate]\n---\nx\n";
        let p = parse_page("T", "", text);

        assert!(p.front_error.is_some());
        assert_eq!(serialize_page(&p), text);
    }

    #[test]
    fn crlf_and_bom_files_are_kept_intact() {
        for text in [
            "---\r\ncategories: [A]\r\n---\r\n\r\nx\r\n",
            "\u{feff}---\ncategories: [A]\n---\n\nx\n",
        ] {
            let p = parse_page("T", "", text);

            assert_eq!(p.body, text);
            assert_eq!(serialize_page(&p), text);
        }
    }
}
