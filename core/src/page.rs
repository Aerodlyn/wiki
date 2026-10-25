use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_yaml_ng::{Mapping, Value};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infobox: Option<Infobox>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,

    #[serde(default, skip_serializing_if = "is_false")]
    pub quicknote: bool,

    #[serde(flatten)]
    pub extra: Mapping, // store unknown keys
}

impl FrontMatter {
    fn is_empty(&self) -> bool {
        *self == FrontMatter::default()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Infobox {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>, // None = show the page title

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>, // file name inside images/

    #[serde(default)]
    pub fields: Vec<Field>,

    #[serde(flatten)]
    pub extra: Mapping, // store unknown keys
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub label: String,
    pub value: String,
}

impl Serialize for Field {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut mapping = Mapping::new();
        mapping.insert(
            Value::String(self.label.clone()),
            Value::String(self.value.clone()),
        );
        mapping.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mapping = Mapping::deserialize(deserializer)?;
        if mapping.len() != 1 {
            return Err(D::Error::custom(
                "each infobox field must be one `Label: value` pair",
            ));
        }

        let (k, v) = mapping.into_iter().next().unwrap();
        Ok(Field {
            label: scalar(k).map_err(D::Error::custom)?,
            value: scalar(v).map_err(D::Error::custom)?,
        })
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

/// Convert a YAML value into a plain text string, if possible. Returns an error if the value is not
/// a string, number, boolean, or null.
fn scalar(value: Value) -> Result<String, &'static str> {
    match value {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Ok(String::new()),
        _ => Err("infobox field value must be plain text (quote values that start with `[[`)"),
    }
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

    const VARN: &str = r#"---
infobox:
    image: varn-banner.jpg
    fields:
        - Capital: "[[Orhold]]"
        - Ruler: "[[Queen Ilsa]]"
        - Founded: 412 AR
        - Religion: The Old Flame
categories: [Nations, Northern Realms]
quicknote: true        # only present on unfinished quick notes
---

Varn is the northernmost realm of the continent, ruled from [[Orhold]]
by [[Queen Ilsa]]. Its largest port is [[Saltmere|the port of Saltmere]].

![[varn-banner.jpg]]

## History

Founded in 412 AR after the collapse of…
"#;

    fn field(l: &str, v: &str) -> Field {
        Field {
            label: l.into(),
            value: v.into(),
        }
    }

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
    fn parses_categories_and_quicknote() {
        let p = parse_page(
            "Varn",
            "Nations",
            "---\ncategories: [Nations, Northern Realms]\nquicknote: true\n---\n\nVarn is cold.",
        );

        assert_eq!(p.front_error, None);
        assert_eq!(p.front.categories, vec!["Nations", "Northern Realms"]);
        assert!(p.front.quicknote);
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
            "---\nquicknote: true\ncategories: [Test]\n---\n\nx\n",
        );
        p.front.quicknote = false;

        assert!(!serialize_page(&p).contains("quicknote"));
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

    #[test]
    fn parses_spec_example() {
        let p = parse_page("Kingdom of Varn", "Nations", VARN);

        assert_eq!(p.front_error, None);

        let ib = p.front.infobox.as_ref().unwrap();

        assert_eq!(ib.image.as_deref(), Some("varn-banner.jpg"));
        assert_eq!(
            ib.fields,
            vec![
                field("Capital", "[[Orhold]]"),
                field("Ruler", "[[Queen Ilsa]]"),
                field("Founded", "412 AR"),
                field("Religion", "The Old Flame"),
            ]
        );
        assert_eq!(p.front.categories, vec!["Nations", "Northern Realms"]);
        assert!(p.front.quicknote);
        assert!(p.body.starts_with("Varn is the northernmost realm"));
        assert!(p.body.ends_with("collapse of…\n"));
    }

    #[test]
    fn spec_example_round_trips() {
        let p = parse_page("Kingdom of Varn", "Nations", VARN);
        let again = reparse(&p);

        assert_eq!(again.front, p.front);
        assert_eq!(again.body, p.body);
    }

    #[test]
    fn repeated_labels_keep_order() {
        let text = "---\ninfobox:\n  fields:\n    - Child: Ana\n    - Child: Bo\n    - Spouse: Cy\n---\n\nx\n";
        let p = parse_page("T", "", text);

        assert_eq!(
            p.front.infobox.as_ref().unwrap().fields,
            vec![
                field("Child", "Ana"),
                field("Child", "Bo"),
                field("Spouse", "Cy")
            ]
        );
        assert_eq!(reparse(&p).front, p.front);
    }

    #[test]
    fn scalar_values_become_text() {
        let text =
            "---\ninfobox:\n  fields:\n    - Founded: 412\n    - Alive: true\n    - Motto:\n---\n";
        let p = parse_page("T", "", text);

        assert_eq!(
            p.front.infobox.as_ref().unwrap().fields,
            vec![
                field("Founded", "412"),
                field("Alive", "true"),
                field("Motto", "")
            ]
        );
    }

    #[test]
    fn tricky_values_survive_save() {
        let mut p = parse_page("T", "", "body\n");
        let vals = [
            "[[Orhold]]",
            "412",
            "true",
            "a: b",
            "- x",
            "#hash",
            "",
            "  padded  ",
            "line\nbreak",
        ];
        p.front.infobox = Some(Infobox {
            fields: vals.iter().map(|v| field("L", v)).collect(),
            ..Default::default()
        });
        let again = reparse(&p);

        assert_eq!(again.front_error, None);
        assert_eq!(again.front, p.front);
        assert_eq!(again.body, "body\n");
    }

    #[test]
    fn unquoted_link_value_falls_back_to_raw() {
        // YAML reads an unquoted [[Orhold]] as a list inside a list
        let text = "---\ninfobox:\n  fields:\n    - Capital: [[Orhold]]\n---\n\nx\n";
        let p = parse_page("T", "", text);

        assert!(p.front_error.is_some());
        assert_eq!(serialize_page(&p), text);
    }

    #[test]
    fn two_pair_field_falls_back_to_raw() {
        let text = "---\ninfobox:\n  fields:\n    - A: 1\n      B: 2\n---\nx\n";
        let p = parse_page("T", "", text);

        assert!(p.front_error.is_some());
        assert_eq!(serialize_page(&p), text);
    }

    #[test]
    fn unknown_keys_survive_save() {
        let text = "---\naliases: [Varn]\ninfobox:\n  caption: Banner\n  fields: []\nrating: 5\n---\n\nx\n";
        let p = parse_page("T", "", text);
        let out = serialize_page(&p);
        assert!(out.contains("aliases:"), "{out}");
        assert!(out.contains("rating: 5"), "{out}");
        assert!(out.contains("caption: Banner"), "{out}");
        assert_eq!(reparse(&p).front, p.front);
    }
}
