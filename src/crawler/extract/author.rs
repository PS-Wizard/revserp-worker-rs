use std::sync::LazyLock;

use scraper::{Html, Selector};
use serde_json::Value;

use super::normalize_text;

static AUTHOR_META_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[name], meta[property]").expect("hardcoded author selector must be valid")
});

pub(super) fn extract_author(document: &Html, json_ld_blocks: &[String]) -> String {
    if let Some(author) = document.select(&AUTHOR_META_SELECTOR).find_map(|meta| {
        let name = meta.value().attr("name").unwrap_or_default();
        let property = meta.value().attr("property").unwrap_or_default();
        if !name.trim().eq_ignore_ascii_case("author")
            && !property.trim().eq_ignore_ascii_case("article:author")
        {
            return None;
        }

        let content = normalize_text(std::iter::once(
            meta.value().attr("content").unwrap_or_default(),
        ));
        (!content.is_empty()).then_some(content)
    }) {
        return author;
    }

    json_ld_blocks
        .iter()
        .find_map(|block| {
            let value = serde_json::from_str(block).ok()?;
            let author = extract_json_ld_value(&value);
            (!author.is_empty()).then_some(author)
        })
        .unwrap_or_default()
}

fn extract_json_ld_value(value: &Value) -> String {
    match value {
        Value::Array(values) => values
            .iter()
            .find_map(|value| {
                let author = extract_json_ld_value(value);
                (!author.is_empty()).then_some(author)
            })
            .unwrap_or_default(),
        Value::Object(object) => {
            if let Some(author) = object
                .get("@graph")
                .and_then(Value::as_array)
                .and_then(|graph| {
                    graph.iter().find_map(|value| {
                        let author = extract_json_ld_value(value);
                        (!author.is_empty()).then_some(author)
                    })
                })
            {
                return author;
            }

            if !is_relevant_type(object.get("@type")) {
                return String::new();
            }

            extract_author_value(object.get("author"))
        }
        _ => String::new(),
    }
}

fn is_relevant_type(value: Option<&Value>) -> bool {
    let is_relevant = |value: &Value| {
        value.as_str().is_some_and(|type_name| {
            matches!(
                type_name.trim(),
                "Article" | "BlogPosting" | "NewsArticle" | "TechArticle" | "WebPage"
            )
        })
    };

    value.is_some_and(|value| match value {
        Value::String(_) => is_relevant(value),
        Value::Array(values) => values.iter().any(is_relevant),
        _ => false,
    })
}

fn extract_author_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(author)) => normalize_text(std::iter::once(author.as_str())),
        Some(Value::Object(object)) => object
            .get("name")
            .and_then(Value::as_str)
            .map(|name| normalize_text(std::iter::once(name)))
            .filter(|name| !name.is_empty())
            .unwrap_or_default(),
        Some(Value::Array(values)) => values
            .iter()
            .find_map(|value| {
                let author = extract_author_value(Some(value));
                (!author.is_empty()).then_some(author)
            })
            .unwrap_or_default(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(html: &str, json_ld_blocks: &[String]) -> String {
        extract_author(&Html::parse_document(html), json_ld_blocks)
    }

    #[test]
    fn meta_author_has_priority_and_normalizes_whitespace() {
        let blocks = vec![r#"{"@type":"Article","author":"JSON-LD"}"#.to_owned()];
        assert_eq!(
            extract(
                r#"<meta name="author" content="  Jane
 Doe ">"#,
                &blocks
            ),
            "Jane Doe"
        );
    }

    #[test]
    fn skips_empty_meta_author() {
        assert_eq!(
            extract(
                r#"<meta name="author" content=" "><meta name="author" content="Later Author">"#,
                &[],
            ),
            "Later Author"
        );
    }

    #[test]
    fn extracts_article_author_property() {
        assert_eq!(
            extract(
                r#"<meta property=" ARTICLE:AUTHOR " content="Property Author">"#,
                &[]
            ),
            "Property Author"
        );
    }

    #[test]
    fn extracts_author_from_graph_in_order() {
        let blocks = vec![r#"{"@graph":[{"@type":"WebPage","author":{"name":"Graph Author"}},{"@type":"Article","author":"Later"}]}"#.to_owned()];
        assert_eq!(extract("", &blocks), "Graph Author");
    }

    #[test]
    fn traverses_top_level_type_and_author_arrays() {
        let blocks = vec![r#"[{"@type":["Thing"," Article "],"author":[{"name":" "},{"name":" Array Author "}]}]"#.to_owned()];
        assert_eq!(extract("", &blocks), "Array Author");
    }

    #[test]
    fn ignores_malformed_and_irrelevant_json_ld() {
        let blocks = vec![
            "{not json}".to_owned(),
            r#"{"@type":"Product","author":"Wrong Type"}"#.to_owned(),
            r#"{"@type":"Article","author":"Right Author"}"#.to_owned(),
        ];
        assert_eq!(extract("", &blocks), "Right Author");
    }
}
