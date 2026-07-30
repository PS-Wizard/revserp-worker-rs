use std::collections::{HashSet, VecDeque};

use anyhow::{Error, Result, ensure};
use futures_util::{StreamExt, stream::FuturesUnordered};
use reqwest::Url;

use crate::crawler::{FetchClient, fetch::FetchResult, fetch_url};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CrawlJob {
    pub url: Url,
    pub depth: usize,
}

pub struct CrawledPage {
    pub job: CrawlJob,
    pub fetch: FetchResult,
}

pub struct CrawlFailure {
    pub job: CrawlJob,
    pub error: Error,
}

#[derive(Default)]
pub struct CrawlReport {
    pub pages: Vec<CrawledPage>,
    pub failures: Vec<CrawlFailure>,
}

pub struct Crawler {
    fetch_client: FetchClient,
    max_depth: usize,
    max_pages: usize,
    max_concurrency: usize,
}

impl CrawlJob {
    pub fn new(root: Url) -> Self {
        Self {
            url: root,
            depth: 0,
        }
    }
}

impl Crawler {
    pub fn new(
        fetch_client: FetchClient,
        max_depth: usize,
        max_pages: usize,
        max_concurrency: usize,
    ) -> Result<Self> {
        ensure!(max_pages > 0, "max_pages must be greater than zero");
        ensure!(
            max_concurrency > 0,
            "max_concurrency must be greater than zero"
        );
        Ok(Self {
            fetch_client,
            max_depth,
            max_pages,
            max_concurrency,
        })
    }

    pub async fn crawl(&self, root: Url) -> CrawlReport {
        let mut queue = VecDeque::from([CrawlJob::new(root.clone())]);
        let mut in_flight = FuturesUnordered::new();
        let mut seen = HashSet::from([root]);
        let mut report = CrawlReport::default();

        while !queue.is_empty() || !in_flight.is_empty() {
            while in_flight.len() < self.max_concurrency {
                let Some(job) = queue.pop_front() else {
                    break;
                };
                let fetch_client = &self.fetch_client;

                in_flight.push(async move {
                    let result = fetch_url(&job.url, fetch_client).await;
                    (job, result)
                });
            }

            let Some((job, result)) = in_flight.next().await else {
                break;
            };

            match result {
                Ok(fetch) => {
                    if fetch.status_code.is_success() && job.depth < self.max_depth {
                        for link in &fetch.links {
                            if !link.internal {
                                continue;
                            }
                            if seen.len() >= self.max_pages {
                                break;
                            }
                            if seen.insert(link.target_url.clone()) {
                                queue.push_back(CrawlJob {
                                    url: link.target_url.clone(),
                                    depth: job.depth + 1,
                                });
                            }
                        }
                    }
                    report.pages.push(CrawledPage { job, fetch });
                }
                Err(error) => report.failures.push(CrawlFailure { job, error }),
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{ErrorKind, Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;

    #[derive(Clone, Copy)]
    struct Route {
        status: u16,
        body: &'static str,
        delay: Duration,
    }

    fn route(status: u16, body: &'static str) -> Route {
        Route {
            status,
            body,
            delay: Duration::ZERO,
        }
    }

    fn delayed_route(body: &'static str) -> Route {
        Route {
            status: 200,
            body,
            delay: Duration::from_millis(50),
        }
    }

    type TestServer = (
        Url,
        Arc<Mutex<Vec<String>>>,
        Arc<AtomicUsize>,
        thread::JoinHandle<()>,
    );

    fn server(routes: HashMap<&'static str, Route>, expected_requests: usize) -> TestServer {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded_requests = Arc::clone(&requests);
        let active_requests = Arc::new(AtomicUsize::new(0));
        let measured_active_requests = Arc::clone(&active_requests);
        let max_active_requests = Arc::new(AtomicUsize::new(0));
        let measured_max_active_requests = Arc::clone(&max_active_requests);

        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut handlers = Vec::new();
            let mut handled = 0;

            while handled < expected_requests && Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(error) => panic!("accept request: {error}"),
                };

                let mut request = [0; 2048];
                let bytes_read = stream.read(&mut request).unwrap();
                assert!(bytes_read > 0);
                let request = String::from_utf8_lossy(&request[..bytes_read]);
                let path = request.split_whitespace().nth(1).unwrap().to_owned();
                recorded_requests.lock().unwrap().push(path.clone());
                handled += 1;

                let route = *routes.get(path.as_str()).unwrap();
                let active_requests = Arc::clone(&measured_active_requests);
                let max_active_requests = Arc::clone(&measured_max_active_requests);
                handlers.push(thread::spawn(move || {
                    let measure_concurrency = !route.delay.is_zero();
                    if measure_concurrency {
                        let active = active_requests.fetch_add(1, Ordering::SeqCst) + 1;
                        max_active_requests.fetch_max(active, Ordering::SeqCst);
                        thread::sleep(route.delay);
                    }

                    if route.status != 0 {
                        let reason = if route.status == 200 {
                            "OK"
                        } else {
                            "Not Found"
                        };
                        let response = format!(
                            "HTTP/1.1 {} {}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            route.status,
                            reason,
                            route.body.len(),
                            route.body
                        );
                        stream.write_all(response.as_bytes()).unwrap();
                    }

                    if measure_concurrency {
                        active_requests.fetch_sub(1, Ordering::SeqCst);
                    }
                }));
            }

            assert_eq!(handled, expected_requests);
            for handler in handlers {
                handler.join().unwrap();
            }
        });

        (
            Url::parse(&format!("http://{address}/")).unwrap(),
            requests,
            max_active_requests,
            handle,
        )
    }

    #[tokio::test]
    async fn crawl_is_breadth_first_deduplicated_internal_and_depth_bounded() {
        let routes = HashMap::from([
            (
                "/",
                route(
                    200,
                    r#"<a href="/a">a</a><a href="/a">duplicate</a><a href="/bad">bad</a><a href="https://external.example/page">external</a>"#,
                ),
            ),
            ("/a", route(200, r#"<a href="/deep">deep</a>"#)),
            ("/bad", route(404, r#"<a href="/never">never</a>"#)),
            ("/deep", route(200, "done")),
        ]);
        let (root, requests, _, server) = server(routes, 4);
        let crawler = Crawler::new(FetchClient::new(), 2, 10, 1).unwrap();

        let report = crawler.crawl(root).await;
        server.join().unwrap();

        assert_eq!(*requests.lock().unwrap(), ["/", "/a", "/bad", "/deep"]);
        assert_eq!(
            report
                .pages
                .iter()
                .map(|page| page.job.depth)
                .collect::<Vec<_>>(),
            [0, 1, 1, 2]
        );
        assert!(report.failures.is_empty());
    }

    #[tokio::test]
    async fn crawl_stops_scheduling_at_page_limit() {
        let routes = HashMap::from([
            (
                "/",
                route(
                    200,
                    r#"<a href="/a">a</a><a href="/b">b</a><a href="/c">c</a>"#,
                ),
            ),
            ("/a", route(200, "done")),
        ]);
        let (root, requests, _, server) = server(routes, 2);
        let crawler = Crawler::new(FetchClient::new(), 2, 2, 1).unwrap();

        let report = crawler.crawl(root).await;
        server.join().unwrap();

        assert_eq!(*requests.lock().unwrap(), ["/", "/a"]);
        assert_eq!(report.pages.len(), 2);
    }

    #[tokio::test]
    async fn crawl_records_fetch_failure_and_continues() {
        let routes = HashMap::from([
            (
                "/",
                route(200, r#"<a href="/fail">fail</a><a href="/ok">ok</a>"#),
            ),
            ("/fail", route(0, "")),
            ("/ok", route(200, "done")),
        ]);
        let (root, requests, _, server) = server(routes, 3);
        let crawler = Crawler::new(FetchClient::new(), 1, 3, 1).unwrap();

        let report = crawler.crawl(root).await;
        server.join().unwrap();

        assert_eq!(*requests.lock().unwrap(), ["/", "/fail", "/ok"]);
        assert_eq!(report.pages.len(), 2);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].job.url.path(), "/fail");
    }

    #[tokio::test]
    async fn crawl_respects_concurrency_limit() {
        let routes = HashMap::from([
            (
                "/",
                route(
                    200,
                    r#"<a href="/a">a</a><a href="/b">b</a><a href="/c">c</a>"#,
                ),
            ),
            ("/a", delayed_route("done")),
            ("/b", delayed_route("done")),
            ("/c", delayed_route("done")),
        ]);
        let (root, _, max_active_requests, server) = server(routes, 4);
        let crawler = Crawler::new(FetchClient::new(), 1, 4, 2).unwrap();

        let report = crawler.crawl(root).await;
        server.join().unwrap();

        assert_eq!(report.pages.len(), 4);
        assert_eq!(max_active_requests.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn crawler_rejects_zero_limits() {
        assert!(Crawler::new(FetchClient::new(), 0, 0, 1).is_err());
        assert!(Crawler::new(FetchClient::new(), 0, 1, 0).is_err());
    }
}
