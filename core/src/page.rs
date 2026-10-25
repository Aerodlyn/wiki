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

pub fn version_of(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

pub fn parse_page(title: &str, folder: &str, text: &str) -> Page {
    Page {
        title: title.to_string(),
        folder: folder.to_string(),
        front: FrontMatter::default(),
        front_error: None,
        body: text.to_string(),
        version: version_of(text),
    }
}

pub fn serialize_page(page: &Page) -> String {
    page.body.clone()
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
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
}
