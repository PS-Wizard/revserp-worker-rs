use std::sync::LazyLock;

use reqwest::Url;
use scraper::{Html, Selector};

use super::normalize_url;

static METADATA_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        r#"html[lang], title, meta[name="description"], link[rel="canonical"], meta[name="viewport"], meta[name="robots"]"#,
    )
    .expect("hardcoded metadata selector must be valid")
});

#[derive(Debug, Default, Eq, PartialEq)]
pub struct ParsedMetadata {
    pub title: String,
    pub meta_description: String,
    pub canonical_url: String,
    pub lang: String,
    pub viewport: String,
    pub robots: String,
}

pub(super) fn extract_metadata(document: &Html, page_url: &Url) -> ParsedMetadata {
    let mut title = None;
    let mut meta_description = None;
    let mut canonical_url = None;
    let mut lang = None;
    let mut viewport = None;
    let mut robots = None;

    for element in document.select(&METADATA_SELECTOR) {
        match element.value().name() {
            "html" if lang.is_none() => {
                lang = Some(trimmed_attribute(&element, "lang"));
            }
            "title" if title.is_none() => {
                title = Some(element.text().collect::<String>().trim().to_owned());
            }
            "link" if canonical_url.is_none() => {
                canonical_url = Some(trimmed_attribute(&element, "href"));
            }
            "meta" => match element.value().attr("name") {
                Some("description") if meta_description.is_none() => {
                    meta_description = Some(trimmed_attribute(&element, "content"));
                }
                Some("viewport") if viewport.is_none() => {
                    viewport = Some(trimmed_attribute(&element, "content"));
                }
                Some("robots") if robots.is_none() => {
                    robots = Some(trimmed_attribute(&element, "content"));
                }
                _ => {}
            },
            _ => {}
        }
    }

    let canonical_url = canonical_url.unwrap_or_default();
    let canonical_url = if canonical_url.is_empty() {
        canonical_url
    } else {
        normalize_url(&canonical_url, page_url)
            .map(|url| url.to_string())
            .unwrap_or(canonical_url)
    };

    ParsedMetadata {
        title: title.unwrap_or_default(),
        meta_description: meta_description.unwrap_or_default(),
        canonical_url,
        lang: lang.unwrap_or_default(),
        viewport: viewport.unwrap_or_default(),
        robots: robots.unwrap_or_default(),
    }
}

fn trimmed_attribute(element: &scraper::ElementRef<'_>, attribute: &str) -> String {
    element
        .value()
        .attr(attribute)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(html: &str) -> ParsedMetadata {
        let document = Html::parse_document(html);
        let page_url = Url::parse("https://example.com/section/page").unwrap();
        extract_metadata(&document, &page_url)
    }

    #[test]
    fn extracts_trimmed_metadata_and_normalizes_canonical_url() {
        let metadata = extract(
            r#"
                <html lang=" en-US ">
                    <head>
                        <title> Test Page </title>
                        <meta name="description" content=" A useful description. ">
                        <link rel="canonical" href="../canonical#section">
                        <meta name="viewport" content=" width=device-width, initial-scale=1 ">
                        <meta name="robots" content=" index,follow ">
                    </head>
                </html>
            "#,
        );

        assert_eq!(
            metadata,
            ParsedMetadata {
                title: "Test Page".to_owned(),
                meta_description: "A useful description.".to_owned(),
                canonical_url: "https://example.com/canonical".to_owned(),
                lang: "en-US".to_owned(),
                viewport: "width=device-width, initial-scale=1".to_owned(),
                robots: "index,follow".to_owned(),
            }
        );
    }

    #[test]
    fn uses_first_matching_element_even_when_its_value_is_empty() {
        let metadata = extract(
            r#"
                <title> </title><title>Ignored</title>
                <meta name="description" content=" ">
                <meta name="description" content="Ignored">
                <link rel="canonical" href="mailto:test@example.com">
            "#,
        );

        assert!(metadata.title.is_empty());
        assert!(metadata.meta_description.is_empty());
        assert_eq!(metadata.canonical_url, "mailto:test@example.com");
    }

    #[test]
    fn returns_empty_fields_when_metadata_is_absent() {
        assert_eq!(
            extract("<html><body>Content</body></html>"),
            ParsedMetadata::default()
        );
    }
}
