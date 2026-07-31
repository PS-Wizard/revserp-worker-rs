use std::{collections::HashMap, sync::LazyLock};

use scraper::{Html, Selector};

static SOCIAL_METADATA_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[property], meta[name]")
        .expect("hardcoded social metadata selector must be valid")
});

#[derive(Debug, Default, Eq, PartialEq)]
pub struct ParsedSocialMetadata {
    pub open_graph: HashMap<String, String>,
    pub twitter: HashMap<String, String>,
}

pub(super) fn extract_social_metadata(document: &Html) -> ParsedSocialMetadata {
    let mut social_metadata = ParsedSocialMetadata::default();

    for element in document.select(&SOCIAL_METADATA_SELECTOR) {
        let content = element
            .value()
            .attr("content")
            .map(str::trim)
            .filter(|content| !content.is_empty());
        let Some(content) = content else {
            continue;
        };

        if let Some(property) = element.value().attr("property").map(str::trim)
            && property
                .get(..3)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("og:"))
        {
            social_metadata
                .open_graph
                .insert(property.to_owned(), content.to_owned());
        }

        if let Some(name) = element.value().attr("name").map(str::trim)
            && name
                .get(..8)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("twitter:"))
        {
            social_metadata
                .twitter
                .insert(name.to_owned(), content.to_owned());
        }
    }

    social_metadata
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(html: &str) -> ParsedSocialMetadata {
        extract_social_metadata(&Html::parse_document(html))
    }

    #[test]
    fn extracts_trimmed_case_insensitive_social_metadata() {
        assert_eq!(
            extract(
                r#"
                    <meta property="  OG:Title  " content="  Open graph title  ">
                    <meta name=" Twitter:Card " content=" summary " property=" og:type ">
                "#
            ),
            ParsedSocialMetadata {
                open_graph: HashMap::from([
                    ("OG:Title".to_owned(), "Open graph title".to_owned()),
                    ("og:type".to_owned(), "summary".to_owned()),
                ]),
                twitter: HashMap::from([("Twitter:Card".to_owned(), "summary".to_owned())]),
            }
        );
    }

    #[test]
    fn filters_empty_and_unrelated_values_and_last_duplicate_wins() {
        assert_eq!(
            extract(
                r#"
                    <meta property="og:empty" content=" ">
                    <meta property="description" content="unrelated">
                    <meta name="twitter:empty" content="">
                    <meta name="description" content="unrelated">
                    <meta property="og:title" content="first">
                    <meta property="og:title" content=" second ">
                    <meta name="twitter:card" content="first">
                    <meta name="twitter:card" content=" second ">
                "#
            ),
            ParsedSocialMetadata {
                open_graph: HashMap::from([("og:title".to_owned(), "second".to_owned())]),
                twitter: HashMap::from([("twitter:card".to_owned(), "second".to_owned())]),
            }
        );
    }
}
