use crate::crawler::facts::PageFact;

use super::super::{DerivedIssue, IssueType, Pillar, Severity};

const SHORT_TITLE_CHARACTER_THRESHOLD: usize = 30;
const LONG_TITLE_CHARACTER_THRESHOLD: usize = 60;
const SHORT_META_DESCRIPTION_CHARACTER_THRESHOLD: usize = 120;
const LONG_META_DESCRIPTION_CHARACTER_THRESHOLD: usize = 160;

const MISSING_TITLE: IssueType = IssueType::new(Pillar::Seo, "serp_metadata", "missing_title");
const TITLE_TOO_LONG: IssueType = IssueType::new(Pillar::Seo, "serp_metadata", "title_too_long");
const TITLE_TOO_SHORT: IssueType = IssueType::new(Pillar::Seo, "serp_metadata", "title_too_short");
const MISSING_META_DESCRIPTION: IssueType =
    IssueType::new(Pillar::Seo, "serp_metadata", "missing_meta_description");
const META_DESCRIPTION_TOO_LONG: IssueType =
    IssueType::new(Pillar::Seo, "serp_metadata", "meta_description_too_long");
const META_DESCRIPTION_TOO_SHORT: IssueType =
    IssueType::new(Pillar::Seo, "serp_metadata", "meta_description_too_short");

pub(super) fn derive(page: &PageFact) -> Vec<DerivedIssue> {
    let mut issues = Vec::new();
    let title_length = page.title.trim().len();
    if title_length == 0 {
        issues.push(DerivedIssue::new(
            &page.url,
            MISSING_TITLE,
            Severity::High,
            "Page is missing a title",
            "Add a descriptive <title> tag.".to_owned(),
        ));
    } else {
        if title_length > LONG_TITLE_CHARACTER_THRESHOLD {
            issues.push(DerivedIssue::new(
                &page.url,
                TITLE_TOO_LONG,
                Severity::Medium,
                "Page title is too long",
                format!("Title is {title_length} characters (recommended: 30-60)."),
            ));
        }
        if title_length < SHORT_TITLE_CHARACTER_THRESHOLD {
            issues.push(DerivedIssue::new(
                &page.url,
                TITLE_TOO_SHORT,
                Severity::Medium,
                "Page title is too short",
                format!("Title is {title_length} characters (recommended: 30-60)."),
            ));
        }
    }

    let meta_description_length = page.meta_description.trim().len();
    if meta_description_length == 0 {
        issues.push(DerivedIssue::new(
            &page.url,
            MISSING_META_DESCRIPTION,
            Severity::Medium,
            "Page is missing a meta description",
            "Add a meta description summarizing the page content.".to_owned(),
        ));
    } else {
        if meta_description_length > LONG_META_DESCRIPTION_CHARACTER_THRESHOLD {
            issues.push(DerivedIssue::new(
                &page.url,
                META_DESCRIPTION_TOO_LONG,
                Severity::Medium,
                "Meta description is too long",
                format!(
                    "Meta description is {meta_description_length} characters (recommended: 120-160)."
                ),
            ));
        }
        if meta_description_length < SHORT_META_DESCRIPTION_CHARACTER_THRESHOLD {
            issues.push(DerivedIssue::new(
                &page.url,
                META_DESCRIPTION_TOO_SHORT,
                Severity::Medium,
                "Meta description is too short",
                format!(
                    "Meta description is {meta_description_length} characters (recommended: 120-160)."
                ),
            ));
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> PageFact {
        PageFact {
            url: "https://example.com/page".to_owned(),
            content_type: Some("text/html".to_owned()),
            ..PageFact::default()
        }
    }

    fn issue_ids(page: &PageFact) -> Vec<&'static str> {
        derive(page)
            .iter()
            .map(|issue| issue.issue_type.id())
            .collect()
    }

    fn assert_issue(
        issue: &DerivedIssue,
        id: &'static str,
        severity: Severity,
        message: &str,
        details: &str,
    ) {
        assert_eq!(issue.issue_type.id(), id);
        assert_eq!(issue.issue_type.bucket(), "serp_metadata");
        assert_eq!(issue.severity, severity);
        assert_eq!(issue.message, message);
        assert_eq!(issue.details, details);
    }

    #[test]
    fn derives_missing_metadata_in_order() {
        let issues = derive(&page());

        assert_eq!(issues.len(), 2);
        assert_issue(
            &issues[0],
            "missing_title",
            Severity::High,
            "Page is missing a title",
            "Add a descriptive <title> tag.",
        );
        assert_issue(
            &issues[1],
            "missing_meta_description",
            Severity::Medium,
            "Page is missing a meta description",
            "Add a meta description summarizing the page content.",
        );
    }

    #[test]
    fn derives_long_metadata_in_order() {
        let mut page = page();
        page.title = "t".repeat(61);
        page.meta_description = "m".repeat(161);
        let issues = derive(&page);

        assert_eq!(
            issue_ids(&page),
            vec!["title_too_long", "meta_description_too_long"]
        );
        assert_issue(
            &issues[0],
            "title_too_long",
            Severity::Medium,
            "Page title is too long",
            "Title is 61 characters (recommended: 30-60).",
        );
        assert_issue(
            &issues[1],
            "meta_description_too_long",
            Severity::Medium,
            "Meta description is too long",
            "Meta description is 161 characters (recommended: 120-160).",
        );
    }

    #[test]
    fn derives_short_metadata_in_order() {
        let mut page = page();
        page.title = "t".repeat(29);
        page.meta_description = "m".repeat(119);
        let issues = derive(&page);

        assert_eq!(
            issue_ids(&page),
            vec!["title_too_short", "meta_description_too_short"]
        );
        assert_issue(
            &issues[0],
            "title_too_short",
            Severity::Medium,
            "Page title is too short",
            "Title is 29 characters (recommended: 30-60).",
        );
        assert_issue(
            &issues[1],
            "meta_description_too_short",
            Severity::Medium,
            "Meta description is too short",
            "Meta description is 119 characters (recommended: 120-160).",
        );
    }

    #[test]
    fn exact_metadata_boundaries_are_not_flagged() {
        for (title_length, meta_description_length) in [(30, 120), (60, 160)] {
            let mut page = page();
            page.title = "t".repeat(title_length);
            page.meta_description = "m".repeat(meta_description_length);

            assert!(derive(&page).is_empty());
        }
    }
}
