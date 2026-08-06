use crate::crawler::facts::PageFact;

use super::super::{DerivedIssue, IssueType, Pillar, Severity};

const MISSING_VIEWPORT: IssueType =
    IssueType::new(Pillar::Seo, "technical_seo", "missing_viewport");
const MISSING_LANG: IssueType = IssueType::new(Pillar::Seo, "technical_seo", "missing_lang");

pub(super) fn derive(page: &PageFact) -> Vec<DerivedIssue> {
    let mut issues = Vec::new();

    if page.viewport.trim().is_empty() {
        issues.push(DerivedIssue::new(
            &page.url,
            MISSING_VIEWPORT,
            Severity::High,
            "Page is missing a viewport meta tag",
            "Add a viewport meta tag for mobile optimization.".to_owned(),
        ));
    }
    if page.lang.trim().is_empty() {
        issues.push(DerivedIssue::new(
            &page.url,
            MISSING_LANG,
            Severity::Medium,
            "Page is missing a language attribute",
            "Add a lang attribute to the HTML element.".to_owned(),
        ));
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(viewport: &str, lang: &str) -> PageFact {
        PageFact {
            url: "https://example.com/page".to_owned(),
            viewport: viewport.to_owned(),
            lang: lang.to_owned(),
            ..PageFact::default()
        }
    }

    #[test]
    fn derives_missing_viewport_and_language() {
        let issues = derive(&page("  ", ""));

        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.issue_type.id())
                .collect::<Vec<_>>(),
            ["missing_viewport", "missing_lang"],
        );
        assert_eq!(issues[0].severity, Severity::High);
        assert_eq!(issues[1].severity, Severity::Medium);
    }

    #[test]
    fn ignores_present_viewport_and_language() {
        assert!(derive(&page("width=device-width", "en")).is_empty());
    }
}
