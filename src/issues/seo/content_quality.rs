use crate::crawler::facts::PageFact;

use super::super::{DerivedIssue, IssueType, Pillar, Severity};

const THIN_CONTENT_WORD_COUNT_THRESHOLD: usize = 150;
const NEAR_EMPTY_VISIBLE_CONTENT_WORD_THRESHOLD: usize = 25;

const THIN_CONTENT: IssueType = IssueType::new(Pillar::Seo, "content_quality", "thin_content");
const NEAR_EMPTY_VISIBLE_CONTENT: IssueType =
    IssueType::new(Pillar::Seo, "content_quality", "near_empty_visible_content");

pub(super) fn derive(page: &PageFact) -> Vec<DerivedIssue> {
    let mut issues = Vec::new();
    if page.word_count > 0 && page.word_count < THIN_CONTENT_WORD_COUNT_THRESHOLD {
        issues.push(DerivedIssue::new(
            &page.url,
            THIN_CONTENT,
            Severity::Medium,
            "Page content is thin",
            "Add more useful page content for users and search engines.".to_owned(),
        ));
    }

    if page.word_count < NEAR_EMPTY_VISIBLE_CONTENT_WORD_THRESHOLD {
        let (severity, message, details) = if page.word_count == 0 {
            (
                Severity::High,
                "Page has empty visible content",
                "Page has no meaningful visible text content.".to_owned(),
            )
        } else {
            (
                Severity::Medium,
                "Page has near-empty visible content",
                format!("Page only has {} visible word(s).", page.word_count),
            )
        };
        issues.push(DerivedIssue::new(
            &page.url,
            NEAR_EMPTY_VISIBLE_CONTENT,
            severity,
            message,
            details,
        ));
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> PageFact {
        PageFact {
            url: "https://example.com/page".to_owned(),
            ..PageFact::default()
        }
    }

    fn issue_ids(page: &PageFact) -> Vec<&'static str> {
        derive(page)
            .iter()
            .map(|issue| issue.issue_type.id())
            .collect()
    }

    #[test]
    fn thin_content_threshold_boundaries() {
        for (word_count, expected) in [(0, false), (1, true), (149, true), (150, false)] {
            let mut page = page();
            page.word_count = word_count;
            assert_eq!(issue_ids(&page).contains(&"thin_content"), expected);
        }
    }

    #[test]
    fn near_empty_visible_content_threshold_boundaries() {
        for (word_count, expected) in [(1, true), (24, true), (25, false)] {
            let mut page = page();
            page.word_count = word_count;
            assert_eq!(
                issue_ids(&page).contains(&"near_empty_visible_content"),
                expected
            );
        }
    }

    #[test]
    fn empty_visible_content_has_high_severity_and_exact_copy() {
        let issues = derive(&page());

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type.id(), "near_empty_visible_content");
        assert_eq!(issues[0].issue_type.bucket(), "content_quality");
        assert_eq!(issues[0].severity, Severity::High);
        assert_eq!(issues[0].message, "Page has empty visible content");
        assert_eq!(
            issues[0].details,
            "Page has no meaningful visible text content."
        );
    }

    #[test]
    fn thin_content_precedes_near_empty_visible_content() {
        let mut page = page();
        page.word_count = 1;

        assert_eq!(
            issue_ids(&page),
            vec!["thin_content", "near_empty_visible_content"]
        );
    }
}
