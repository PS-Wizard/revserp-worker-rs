use std::time::{Duration, Instant};

use anyhow::Result;
use crawler::{Crawler, FetchClient};

mod crawler;

#[tokio::main]
async fn main() -> Result<()> {
    let root = "https://revketer.ai".parse()?;
    let crawler = Crawler::new(FetchClient::new(), 1, 70, 10)?;
    let crawl_started = Instant::now();
    let report = crawler.crawl(root).await;
    let crawl_wall_time = crawl_started.elapsed();

    let page_count = report.pages.len();
    let failure_count = report.failures.len();
    let mut total_time_to_headers = Duration::ZERO;
    let mut total_body_download_time = Duration::ZERO;
    let mut total_page_extraction_time = Duration::ZERO;
    let mut total_response_size = 0;
    let mut total_visible_text_bytes = 0;

    for page in report.pages {
        let fetch_time = page.fetch.time_to_headers + page.fetch.body_download_time;
        total_time_to_headers += page.fetch.time_to_headers;
        total_body_download_time += page.fetch.body_download_time;
        total_page_extraction_time += page.fetch.page_extraction_time;
        total_response_size += page.fetch.response_size;

        println!("depth {}: {}", page.job.depth, page.job.url);
        println!("  status: {}", page.fetch.status_code);
        println!("  final URL: {}", page.fetch.final_url);
        println!("  content type: {:?}", page.fetch.content_type);
        println!("  response size: {} bytes", page.fetch.response_size);
        println!("  ETag: {:?}", page.fetch.etag);
        println!("  Last-Modified: {:?}", page.fetch.last_modified);
        println!("  Retry-After: {:?}", page.fetch.retry_after);
        println!("  time-to-headers (TTFB): {:?}", page.fetch.time_to_headers);
        println!("  body download: {:?}", page.fetch.body_download_time);
        println!("  total fetch: {fetch_time:?}");
        println!("  page extraction: {:?}", page.fetch.page_extraction_time);

        if let Some(page) = page.fetch.page.as_ref() {
            total_visible_text_bytes += page.visible_text.len();
            let visible_text_preview: String = page.visible_text.chars().collect();

            println!("  title: {:?}", page.metadata.title);
            println!("  author: {:?}", page.author);
            println!("  meta description: {:?}", page.metadata.meta_description);
            println!("  canonical: {:?}", page.metadata.canonical_url);
            println!("  language: {:?}", page.metadata.lang);
            println!("  viewport: {:?}", page.metadata.viewport);
            println!("  robots: {:?}", page.metadata.robots);
            println!("  open graph: {:?}", page.social_metadata.open_graph);
            println!("  twitter: {:?}", page.social_metadata.twitter);
            for json_ld_block in &page.structured_data.json_ld_blocks {
                println!("  JSON-LD: {json_ld_block}");
            }
            println!(
                "  visible text ({} bytes): {:?}",
                page.visible_text.len(),
                visible_text_preview
            );
            println!("  image count: {}", page.images.count);
            println!("  images without alt: {}", page.images.without_alt_count);
            println!(
                "  images without dimensions: {}",
                page.images.without_dimensions_count
            );
            println!("  h1 count: {}", page.headings.h1_count);
            for heading in &page.headings.outline {
                println!("  h{}: {}", heading.level, heading.text);
            }
            for link in &page.links {
                println!("  {link:?}");
            }
        }
    }

    for failure in report.failures {
        eprintln!(
            "failed depth {} {}: {:#}",
            failure.job.depth, failure.job.url, failure.error
        );
    }

    let total_fetch_time = total_time_to_headers + total_body_download_time;
    let total_measured_work = total_fetch_time + total_page_extraction_time;

    println!("\ncrawl totals:");
    println!("  pages fetched: {page_count}");
    println!("  pages failed: {failure_count}");
    println!("  response body: {total_response_size} bytes");
    println!("  visible text: {total_visible_text_bytes} bytes");
    println!("  summed TTFB: {total_time_to_headers:?}");
    println!("  summed body download: {total_body_download_time:?}");
    println!("  summed fetch: {total_fetch_time:?}");
    println!("  summed page extraction: {total_page_extraction_time:?}");
    println!("  summed measured work: {total_measured_work:?}");
    println!("  crawl wall time: {crawl_wall_time:?}");

    Ok(())
}
