use crate::crawler::facts::PageFact;

use super::super::{DerivedIssue, IssueType, Pillar, Severity};

const IMAGES_MISSING_ALT: IssueType =
    IssueType::new(Pillar::Seo, "media_optimization", "images_missing_alt");
const IMAGES_MISSING_DIMENSIONS: IssueType = IssueType::new(
    Pillar::Seo,
    "media_optimization",
    "images_missing_dimensions",
);
const TOO_MANY_IMAGES_ON_PAGE: IssueType =
    IssueType::new(Pillar::Seo, "media_optimization", "too_many_images_on_page");

const MINIMUM_IMAGE_COUNT: usize = 10;
const WORDS_PER_IMAGE_THRESHOLD: usize = 50;

pub(super) fn derive(page: &PageFact) -> Vec<DerivedIssue> {
    let mut issues = Vec::new();

    if page.image_count > 0 && page.images_without_alt > 0 {
        issues.push(DerivedIssue::new(
            &page.url,
            IMAGES_MISSING_ALT,
            Severity::Low,
            "Page has images missing alt text",
            "Add descriptive alt text to meaningful images.".to_owned(),
        ));
    }
    if page.image_count > 0 && page.images_without_dimensions > 0 {
        issues.push(DerivedIssue::new(
            &page.url,
            IMAGES_MISSING_DIMENSIONS,
            Severity::Low,
            "Page has images missing dimensions",
            "Set explicit image width and height attributes where possible.".to_owned(),
        ));
    }
    if page.image_count >= MINIMUM_IMAGE_COUNT
        && (page.word_count == 0 || page.word_count / page.image_count < WORDS_PER_IMAGE_THRESHOLD)
    {
        issues.push(DerivedIssue::new(
            &page.url,
            TOO_MANY_IMAGES_ON_PAGE,
            Severity::Low,
            "Page may have too many images for its content length",
            format!(
                "Page has {} images and {} words, which is fewer than {} words per image.",
                page.image_count, page.word_count, WORDS_PER_IMAGE_THRESHOLD
            ),
        ));
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(image_count: usize, word_count: usize) -> PageFact {
        PageFact {
            url: "https://example.com/page".to_owned(),
            image_count,
            word_count,
            ..PageFact::default()
        }
    }

    #[test]
    fn derives_all_media_optimization_issues() {
        let mut page = page(10, 0);
        page.images_without_alt = 2;
        page.images_without_dimensions = 3;

        let issues = derive(&page);

        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.issue_type.id())
                .collect::<Vec<_>>(),
            [
                "images_missing_alt",
                "images_missing_dimensions",
                "too_many_images_on_page"
            ],
        );
        assert!(issues.iter().all(|issue| issue.severity == Severity::Low));
        assert_eq!(
            issues[2].details,
            "Page has 10 images and 0 words, which is fewer than 50 words per image."
        );
    }

    #[test]
    fn respects_image_and_words_per_image_boundaries() {
        let mut complete = page(9, 0);
        complete.images_without_alt = 0;
        complete.images_without_dimensions = 0;
        assert!(derive(&complete).is_empty());

        assert!(derive(&page(10, 500)).is_empty());
        assert_eq!(derive(&page(10, 499)).len(), 1);
    }

    #[test]
    fn only_flags_missing_attributes_when_images_exist() {
        let mut page = page(0, 0);
        page.images_without_alt = 1;
        page.images_without_dimensions = 1;

        assert!(derive(&page).is_empty());
    }
}
