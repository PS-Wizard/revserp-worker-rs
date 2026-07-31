use std::sync::LazyLock;

use scraper::{Html, Selector};

static IMAGE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img").expect("hardcoded image selector must be valid"));

#[derive(Debug, Default, Eq, PartialEq)]
pub struct ParsedImages {
    pub count: usize,
    pub without_alt_count: usize,
    pub without_dimensions_count: usize,
}

pub(super) fn extract_images(document: &Html) -> ParsedImages {
    let mut images = ParsedImages::default();

    for image in document.select(&IMAGE_SELECTOR) {
        images.count += 1;
        if image
            .value()
            .attr("alt")
            .is_none_or(|value| value.trim().is_empty())
        {
            images.without_alt_count += 1;
        }
        if image
            .value()
            .attr("width")
            .is_none_or(|value| value.trim().is_empty())
            || image
                .value()
                .attr("height")
                .is_none_or(|value| value.trim().is_empty())
        {
            images.without_dimensions_count += 1;
        }
    }

    images
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(html: &str) -> ParsedImages {
        extract_images(&Html::parse_document(html))
    }

    #[test]
    fn counts_complete_images() {
        assert_eq!(
            extract(r#"<img alt="hero" width="640" height="480">"#),
            ParsedImages {
                count: 1,
                without_alt_count: 0,
                without_dimensions_count: 0,
            }
        );
    }

    #[test]
    fn counts_missing_empty_and_whitespace_alt() {
        assert_eq!(
            extract(
                r#"
                    <img width="1" height="1">
                    <img alt="" width="1" height="1">
                    <img alt="   " width="1" height="1">
                "#,
            ),
            ParsedImages {
                count: 3,
                without_alt_count: 3,
                without_dimensions_count: 0,
            }
        );
    }

    #[test]
    fn counts_missing_and_empty_dimensions_when_either_is_missing() {
        assert_eq!(
            extract(
                r#"
                    <img alt="one" width="1">
                    <img alt="two" height="1">
                    <img alt="three" width="" height="1">
                    <img alt="four" width="1" height=" ">
                    <img alt="five">
                "#,
            ),
            ParsedImages {
                count: 5,
                without_alt_count: 0,
                without_dimensions_count: 5,
            }
        );
    }

    #[test]
    fn does_not_validate_dimension_values() {
        assert_eq!(
            extract(r#"<img alt="image" width="not-a-number" height="auto">"#),
            ParsedImages {
                count: 1,
                without_alt_count: 0,
                without_dimensions_count: 0,
            }
        );
    }

    #[test]
    fn returns_zero_counts_when_no_images_exist() {
        assert_eq!(extract("<p>No images</p>"), ParsedImages::default());
    }
}
