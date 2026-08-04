use crate::crawler::facts::CrawlFacts;

mod seo;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum Pillar {
    Seo,
    Aeo,
    PageSpeed,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Severity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct IssueType {
    pillar: Pillar,
    bucket: &'static str,
    id: &'static str,
}

impl IssueType {
    const fn new(pillar: Pillar, bucket: &'static str, id: &'static str) -> Self {
        Self { pillar, bucket, id }
    }

    pub(crate) const fn pillar(self) -> Pillar {
        self.pillar
    }

    pub(crate) const fn bucket(self) -> &'static str {
        self.bucket
    }

    pub(crate) const fn id(self) -> &'static str {
        self.id
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DerivedIssue {
    pub(crate) url: String,
    pub(crate) issue_type: IssueType,
    pub(crate) severity: Severity,
    pub(crate) message: String,
    pub(crate) details: String,
}

impl DerivedIssue {
    fn new(
        url: &str,
        issue_type: IssueType,
        severity: Severity,
        message: &str,
        details: String,
    ) -> Self {
        Self {
            url: url.to_owned(),
            issue_type,
            severity,
            message: message.to_owned(),
            details,
        }
    }
}

pub(crate) fn derive_issues(facts: &CrawlFacts) -> Vec<DerivedIssue> {
    seo::derive(facts)
}
