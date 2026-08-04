use std::{collections::HashMap, time::Duration};

use super::runner::CrawlReport;

#[derive(Debug, Default)]
pub(crate) struct PageFact {
    // Crawl and response
    pub(crate) url: String,
    pub(crate) depth: usize,
    pub(crate) status_code: u16,
    pub(crate) content_type: Option<String>,
    pub(crate) response_size: usize,
    pub(crate) response_time: Duration,
    pub(crate) fetch_error: Option<String>,
    pub(crate) soft_404: bool,

    // Metadata
    pub(crate) title: String,
    pub(crate) meta_description: String,
    pub(crate) author: String,
    pub(crate) canonical_url: String,
    pub(crate) lang: String,
    pub(crate) viewport: String,
    pub(crate) robots: String,

    // Content
    pub(crate) primary_h1: String,
    pub(crate) h1_count: usize,
    pub(crate) h2_count: usize,
    pub(crate) heading_outline: Vec<(u8, String)>,
    pub(crate) word_count: usize,
    pub(crate) visible_text: String,

    // Media and links
    pub(crate) image_count: usize,
    pub(crate) images_without_alt: usize,
    pub(crate) images_without_dimensions: usize,
    pub(crate) external_link_count: usize,

    // AEO
    pub(crate) open_graph: HashMap<String, String>,
    pub(crate) json_ld_blocks: Vec<String>,
}

impl PageFact {
    pub(crate) fn is_healthy(&self) -> bool {
        self.status_code < 400 && !self.soft_404 && self.fetch_error.is_none()
    }

    pub(crate) fn is_scoreable_content_type(&self) -> bool {
        match self.content_type.as_deref().map(str::trim) {
            None | Some("") => true,
            Some(content_type) => content_type.to_ascii_lowercase().contains("text/html"),
        }
    }
}

#[derive(Debug)]
pub(crate) struct LinkFact {
    pub(crate) source_url: String,
    pub(crate) target_url: String,
    pub(crate) target_status: Option<u16>,
}

#[derive(Debug)]
pub(crate) struct CrawlFacts {
    pub(crate) pages: Vec<PageFact>,
    pub(crate) links: Vec<LinkFact>,
}

impl From<CrawlReport> for CrawlFacts {
    fn from(report: CrawlReport) -> Self {
        let status_by_url: HashMap<String, u16> = report
            .pages
            .iter()
            .flat_map(|page| {
                let status = page.fetch.status_code.as_u16();
                [
                    (page.job.url.to_string(), status),
                    (page.fetch.final_url.to_string(), status),
                ]
            })
            .collect();
        let mut pages = Vec::with_capacity(report.pages.len() + report.failures.len());
        let mut links = Vec::new();

        for crawled_page in report.pages {
            let fetch = crawled_page.fetch;
            let page = fetch.page.unwrap_or_default();

            let url = fetch.final_url.to_string();
            let external_link_count = page.links.iter().filter(|link| !link.internal).count();
            links.extend(page.links.iter().filter(|link| link.internal).map(|link| {
                let target_url = link.target_url.to_string();
                LinkFact {
                    source_url: url.clone(),
                    target_status: status_by_url.get(&target_url).copied(),
                    target_url,
                }
            }));

            let word_count = page.visible_text.split_whitespace().count();
            pages.push(PageFact {
                url,
                depth: crawled_page.job.depth,
                status_code: fetch.status_code.as_u16(),
                content_type: fetch.content_type,
                response_size: fetch.response_size,
                response_time: fetch.time_to_headers + fetch.body_download_time,
                fetch_error: None,
                soft_404: false,
                title: page.metadata.title,
                meta_description: page.metadata.meta_description,
                author: page.author,
                canonical_url: page.metadata.canonical_url,
                lang: page.metadata.lang,
                viewport: page.metadata.viewport,
                robots: page.metadata.robots,
                primary_h1: page.headings.primary_h1,
                h1_count: page.headings.h1_count,
                h2_count: page.headings.h2_count,
                heading_outline: page
                    .headings
                    .outline
                    .into_iter()
                    .map(|heading| (heading.level, heading.text))
                    .collect(),
                word_count,
                visible_text: page.visible_text,
                image_count: page.images.count,
                images_without_alt: page.images.without_alt_count,
                images_without_dimensions: page.images.without_dimensions_count,
                external_link_count,
                open_graph: page.social_metadata.open_graph,
                json_ld_blocks: page.structured_data.json_ld_blocks,
            });
        }

        pages.extend(report.failures.into_iter().map(|failure| PageFact {
            url: failure.job.url.to_string(),
            depth: failure.job.depth,
            fetch_error: Some(format!("{:#}", failure.error)),
            ..PageFact::default()
        }));

        pages.sort_unstable_by(|left, right| left.url.cmp(&right.url));
        links.sort_unstable_by(|left, right| {
            left.source_url
                .cmp(&right.source_url)
                .then_with(|| left.target_url.cmp(&right.target_url))
        });

        Self { pages, links }
    }
}

#[cfg(test)]
mod tests {
    use reqwest::{StatusCode, Url};

    use super::*;
    use crate::crawler::{
        extract::{ExtractedPage, ParsedHeadings, ParsedLink},
        fetch::FetchResult,
        runner::{CrawlFailure, CrawlJob, CrawledPage},
    };

    fn fetch_result(
        final_url: Url,
        status_code: StatusCode,
        page: Option<ExtractedPage>,
    ) -> FetchResult {
        FetchResult {
            status_code,
            final_url,
            content_type: Some("text/html".to_owned()),
            response_size: 512,
            etag: None,
            last_modified: None,
            retry_after: None,
            page,
            javascript_rendered: false,
            javascript_render_time: Duration::ZERO,
            time_to_headers: Duration::from_millis(10),
            body_download_time: Duration::from_millis(5),
            page_extraction_time: Duration::ZERO,
        }
    }

    #[test]
    fn crawl_report_conversion_keeps_successful_broken_and_failed_pages() {
        let root = Url::parse("https://example.com/").unwrap();
        let target = Url::parse("https://example.com/missing").unwrap();
        let external = Url::parse("https://other.example/").unwrap();
        let timeout = Url::parse("https://example.com/timeout").unwrap();
        let extracted_page = ExtractedPage {
            links: vec![
                ParsedLink {
                    target_url: target.clone(),
                    anchor_text: "Missing".to_owned(),
                    internal: true,
                    nofollow: false,
                },
                ParsedLink {
                    target_url: external,
                    anchor_text: "External".to_owned(),
                    internal: false,
                    nofollow: false,
                },
            ],
            headings: ParsedHeadings {
                primary_h1: "Primary heading".to_owned(),
                h1_count: 1,
                h2_count: 2,
                outline: Vec::new(),
            },
            visible_text: "two visible words".to_owned(),
            ..ExtractedPage::default()
        };
        let report = CrawlReport {
            pages: vec![
                CrawledPage {
                    job: CrawlJob {
                        url: root.clone(),
                        depth: 2,
                    },
                    fetch: fetch_result(root.clone(), StatusCode::OK, Some(extracted_page)),
                },
                CrawledPage {
                    job: CrawlJob {
                        url: target.clone(),
                        depth: 3,
                    },
                    fetch: fetch_result(target.clone(), StatusCode::NOT_FOUND, None),
                },
            ],
            failures: vec![CrawlFailure {
                job: CrawlJob {
                    url: timeout.clone(),
                    depth: 4,
                },
                error: anyhow::anyhow!("request timed out"),
            }],
        };

        let facts = CrawlFacts::from(report);

        assert_eq!(facts.pages.len(), 3);
        let page = &facts.pages[0];
        assert_eq!(
            (
                page.url.as_str(),
                page.depth,
                page.status_code,
                page.response_time,
                page.primary_h1.as_str(),
                page.h1_count,
                page.h2_count,
                page.word_count,
                page.external_link_count,
            ),
            (
                root.as_str(),
                2,
                200,
                Duration::from_millis(15),
                "Primary heading",
                1,
                2,
                3,
                1,
            )
        );

        let broken_page = facts
            .pages
            .iter()
            .find(|page| page.url == target.as_str())
            .unwrap();
        assert_eq!(
            (
                broken_page.status_code,
                broken_page.title.as_str(),
                broken_page.fetch_error.as_deref(),
                broken_page.soft_404,
            ),
            (404, "", None, false)
        );

        let failed_page = facts
            .pages
            .iter()
            .find(|page| page.url == timeout.as_str())
            .unwrap();
        assert_eq!(
            (
                failed_page.status_code,
                failed_page.fetch_error.as_deref(),
                failed_page.soft_404,
            ),
            (0, Some("request timed out"), false)
        );
        assert_eq!(facts.links.len(), 1);
        assert_eq!(
            (
                facts.links[0].source_url.as_str(),
                facts.links[0].target_url.as_str(),
                facts.links[0].target_status,
            ),
            (root.as_str(), target.as_str(), Some(404))
        );
    }
}
