mod author;
mod body;
mod headings;
mod images;
mod links;
mod metadata;
mod social_metadata;
mod structured_data;

use reqwest::Url;
use scraper::Html;

pub use headings::ParsedHeadings;
pub use images::ParsedImages;
pub use links::ParsedLink;
pub use metadata::ParsedMetadata;
pub use social_metadata::ParsedSocialMetadata;
pub use structured_data::ParsedStructuredData;

#[derive(Default)]
pub struct ExtractedPage {
    pub links: Vec<ParsedLink>,
    pub headings: ParsedHeadings,
    pub metadata: ParsedMetadata,
    pub social_metadata: ParsedSocialMetadata,
    pub structured_data: ParsedStructuredData,
    pub author: String,
    pub images: ParsedImages,
    pub visible_text: String,
}

pub fn extract_page(html: &[u8], page_url: &Url) -> ExtractedPage {
    let body = String::from_utf8_lossy(html);
    let document = Html::parse_document(&body);

    let structured_data = structured_data::extract_structured_data(&document);
    let author = author::extract_author(&document, &structured_data.json_ld_blocks);

    ExtractedPage {
        links: links::extract_links(&document, page_url),
        headings: headings::extract_headings(&document),
        metadata: metadata::extract_metadata(&document, page_url),
        social_metadata: social_metadata::extract_social_metadata(&document),
        structured_data,
        author,
        images: images::extract_images(&document),
        visible_text: body::extract_visible_text(&document),
    }
}

fn normalize_url(candidate: &str, page_url: &Url) -> Option<Url> {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return None;
    }

    if let Some(authority) = explicit_authority(candidate) {
        let host = authority.split(['/', '?', '#']).next().unwrap_or_default();
        if host.is_empty() {
            return None;
        }
    }

    let mut normalized_url = page_url.join(candidate).ok()?;
    if !matches!(normalized_url.scheme(), "http" | "https") || normalized_url.host_str().is_none() {
        return None;
    }

    normalized_url.set_fragment(None);
    if normalized_url.path().is_empty() {
        normalized_url.set_path("/");
    }
    Some(normalized_url)
}

fn explicit_authority(candidate: &str) -> Option<&str> {
    if let Some(authority) = candidate.strip_prefix("//") {
        return Some(authority);
    }

    for scheme in ["http:", "https:"] {
        if candidate.get(..scheme.len())?.eq_ignore_ascii_case(scheme) {
            return Some(
                candidate[scheme.len()..]
                    .strip_prefix("//")
                    .unwrap_or_default(),
            );
        }
    }

    None
}

#[derive(Default)]
struct TextNormalizer {
    text: String,
    whitespace: bool,
}

impl TextNormalizer {
    fn push(&mut self, chunk: &str) {
        for character in chunk.chars() {
            if character.is_whitespace() {
                if !self.text.is_empty() {
                    self.whitespace = true;
                }
            } else {
                if self.whitespace {
                    self.text.push(' ');
                    self.whitespace = false;
                }
                self.text.push(character);
            }
        }
    }

    fn finish(self) -> String {
        self.text
    }
}

fn normalize_text<'a>(text: impl Iterator<Item = &'a str>) -> String {
    let mut normalizer = TextNormalizer::default();
    for chunk in text {
        normalizer.push(chunk);
    }
    normalizer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_links_and_headings_from_one_document() {
        let page_url = Url::parse("https://example.com/").unwrap();
        let page = extract_page(
            br#"<title>Page metadata</title><meta name="author" content="Fixture Author"><meta property="og:title" content="OG title"><meta name="twitter:card" content="summary"><link rel="canonical" href="/canonical"><h1>Page title</h1> <p>Useful body</p> <img src="hero.jpg" alt="Hero" width="640" height="480"> <img src="missing-height.jpg" alt="" width="100"> <img src="missing-width.jpg" alt="  " height="100"> <script>ignored()</script> <script type="application/ld+json"> {"@type":"WebPage"} </script> <a href="/next">Next</a>"#,
            &page_url,
        );

        assert_eq!(page.headings.h1_count, 1);
        assert_eq!(page.headings.outline[0].text, "Page title");
        assert_eq!(
            page.links[0].target_url.as_str(),
            "https://example.com/next"
        );
        assert_eq!(page.visible_text, "Page title Useful body Next");
        assert_eq!(page.images.count, 3);
        assert_eq!(page.images.without_alt_count, 2);
        assert_eq!(page.images.without_dimensions_count, 2);
        assert_eq!(
            page.structured_data.json_ld_blocks,
            vec![r#"{"@type":"WebPage"}"#]
        );
        assert_eq!(page.metadata.title, "Page metadata");
        assert_eq!(page.author, "Fixture Author");
        assert_eq!(page.metadata.canonical_url, "https://example.com/canonical");
        assert_eq!(page.social_metadata.open_graph["og:title"], "OG title");
        assert_eq!(page.social_metadata.twitter["twitter:card"], "summary");
    }
}
