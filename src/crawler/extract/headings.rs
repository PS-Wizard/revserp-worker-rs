use std::sync::LazyLock;

use scraper::{Html, Selector};

use super::normalize_text;

static HEADING_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("h1, h2, h3, h4, h5, h6").expect("hardcoded heading selector must be valid")
});

#[derive(Debug, Eq, PartialEq)]
pub struct ParsedHeading {
    pub level: u8,
    pub text: String,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct ParsedHeadings {
    pub h1_count: usize,
    pub outline: Vec<ParsedHeading>,
}

pub(super) fn extract_headings(document: &Html) -> ParsedHeadings {
    let mut headings = ParsedHeadings::default();

    for heading in document.select(&HEADING_SELECTOR) {
        let level = match heading.value().name() {
            "h1" => 1,
            "h2" => 2,
            "h3" => 3,
            "h4" => 4,
            "h5" => 5,
            "h6" => 6,
            _ => continue,
        };

        if level == 1 {
            headings.h1_count += 1;
        }

        let text = normalize_text(heading.text());
        if !text.is_empty() {
            headings.outline.push(ParsedHeading { level, text });
        }
    }

    headings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(html: &str) -> ParsedHeadings {
        extract_headings(&Html::parse_document(html))
    }

    #[test]
    fn preserves_heading_levels_text_and_document_order() {
        let headings = extract(
            r#"
                <h1> Main <span>heading</span> </h1>
                <h3>Skipped level</h3>
                <h2> Back to section </h2>
                <h6>Deep detail</h6>
            "#,
        );

        assert_eq!(
            headings.outline,
            [
                ParsedHeading {
                    level: 1,
                    text: "Main heading".to_owned(),
                },
                ParsedHeading {
                    level: 3,
                    text: "Skipped level".to_owned(),
                },
                ParsedHeading {
                    level: 2,
                    text: "Back to section".to_owned(),
                },
                ParsedHeading {
                    level: 6,
                    text: "Deep detail".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn counts_empty_h1_elements_but_omits_empty_outline_entries() {
        let headings = extract("<h1>First</h1><h1> </h1><h2></h2>");

        assert_eq!(headings.h1_count, 2);
        assert_eq!(
            headings.outline,
            [ParsedHeading {
                level: 1,
                text: "First".to_owned(),
            }]
        );
    }

    #[test]
    fn ignores_non_heading_elements() {
        let headings = extract("<title>Title</title><p>Body</p>");

        assert_eq!(headings, ParsedHeadings::default());
    }
}
