use std::time::{Duration, Instant};

use anyhow::Result;
use crawler::{Crawler, FetchClient};

mod crawler;

#[tokio::main]
async fn main() -> Result<()> {
    let root = "https://revketer.ai/".parse()?;
    let crawler = Crawler::new(FetchClient::new(), 1, 1)?;
    let crawl_started = Instant::now();
    let report = crawler.crawl(root).await;
    let crawl_wall_time = crawl_started.elapsed();

    let page_count = report.pages.len();
    let failure_count = report.failures.len();
    let mut total_time_to_headers = Duration::ZERO;
    let mut total_body_download_time = Duration::ZERO;
    let mut total_page_extraction_time = Duration::ZERO;
    let mut total_visible_text_bytes = 0;

    for page in report.pages {
        let fetch_time = page.fetch.time_to_headers + page.fetch.body_download_time;
        total_time_to_headers += page.fetch.time_to_headers;
        total_body_download_time += page.fetch.body_download_time;
        total_page_extraction_time += page.fetch.page_extraction_time;
        total_visible_text_bytes += page.fetch.visible_text.len();
        let visible_text_preview: String = page.fetch.visible_text.chars().collect();

        println!("depth {}: {}", page.job.depth, page.job.url);
        println!("  time-to-headers (TTFB): {:?}", page.fetch.time_to_headers);
        println!("  body download: {:?}", page.fetch.body_download_time);
        println!("  total fetch: {fetch_time:?}");
        println!("  page extraction: {:?}", page.fetch.page_extraction_time);
        println!(
            "  visible text ({} bytes): {:?}",
            page.fetch.visible_text.len(),
            visible_text_preview
        );
        println!("  h1 count: {}", page.fetch.headings.h1_count);
        for heading in page.fetch.headings.outline {
            println!("  h{}: {}", heading.level, heading.text);
        }
        for link in page.fetch.links {
            println!("  {link:?}");
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
    println!("  visible text: {total_visible_text_bytes} bytes");
    println!("  summed TTFB: {total_time_to_headers:?}");
    println!("  summed body download: {total_body_download_time:?}");
    println!("  summed fetch: {total_fetch_time:?}");
    println!("  summed page extraction: {total_page_extraction_time:?}");
    println!("  summed measured work: {total_measured_work:?}");
    println!("  crawl wall time: {crawl_wall_time:?}");

    Ok(())
}
