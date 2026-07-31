use std::sync::LazyLock;

use scraper::{Html, Selector};

static JSON_LD_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"script[type="application/ld+json"]"#)
        .expect("hardcoded JSON-LD selector must be valid")
});

#[derive(Debug, Default, Eq, PartialEq)]
pub struct ParsedStructuredData {
    pub json_ld_blocks: Vec<String>,
}

pub(super) fn extract_structured_data(document: &Html) -> ParsedStructuredData {
    ParsedStructuredData {
        json_ld_blocks: document
            .select(&JSON_LD_SELECTOR)
            .filter_map(|script| {
                let block = script.text().collect::<String>();
                let block = block.trim();
                (!block.is_empty()).then(|| block.to_owned())
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(html: &str) -> ParsedStructuredData {
        extract_structured_data(&Html::parse_document(html))
    }

    #[test]
    fn retains_multiple_blocks_in_document_order_and_trims_them() {
        assert_eq!(
            extract(
                r#"
                    <script type="application/ld+json"> {"first":1} </script>
                    <script type="application/ld+json">
                        {"second":2}
                    </script>
                "#,
            ),
            ParsedStructuredData {
                json_ld_blocks: vec![r#"{"first":1}"#.to_owned(), r#"{"second":2}"#.to_owned()],
            }
        );
    }

    #[test]
    fn ignores_empty_and_wrong_type_scripts() {
        assert_eq!(
            extract(
                r#"
                    <script type="application/ld+json"> </script>
                    <script type="application/json">{"wrong":true}</script>
                    <script>{"wrong":true}</script>
                "#,
            ),
            ParsedStructuredData::default()
        );
    }

    #[test]
    fn retains_malformed_json_unchanged() {
        assert_eq!(
            extract(r#"<script type="application/ld+json"> {not json} </script>"#).json_ld_blocks,
            vec!["{not json}".to_owned()]
        );
    }
}
