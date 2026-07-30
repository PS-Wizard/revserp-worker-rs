mod body;
mod headings;
mod links;

use reqwest::Url;
use scraper::Html;

pub use headings::ParsedHeadings;
pub use links::ParsedLink;

#[derive(Default)]
pub struct ExtractedPage {
    pub links: Vec<ParsedLink>,
    pub headings: ParsedHeadings,
    pub visible_text: String,
}

pub fn extract_page(html: &[u8], page_url: &Url) -> ExtractedPage {
    let body = String::from_utf8_lossy(html);
    let document = Html::parse_document(&body);

    ExtractedPage {
        links: links::extract_links(&document, page_url),
        headings: headings::extract_headings(&document),
        visible_text: body::extract_visible_text(&document),
    }
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
            br#"<h1>Page title</h1> <p>Useful body</p> <script>ignored()</script> <a href="/next">Next</a>"#,
            &page_url,
        );

        assert_eq!(page.headings.h1_count, 1);
        assert_eq!(page.headings.outline[0].text, "Page title");
        assert_eq!(
            page.links[0].target_url.as_str(),
            "https://example.com/next"
        );
        assert_eq!(page.visible_text, "Page title Useful body Next");
    }
}
