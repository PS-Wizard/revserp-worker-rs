use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::{StatusCode, Url};

use crate::crawler::{
    client::FetchClient,
    extract::{ExtractedPage, extract_page},
};

pub struct FetchResult {
    pub status_code: StatusCode,
    pub final_url: Url,
    pub content_type: Option<String>,
    pub response_size: usize,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub retry_after: Option<String>,
    pub page: Option<ExtractedPage>,
    pub time_to_headers: Duration,
    pub body_download_time: Duration,
    pub page_extraction_time: Duration,
}

pub(super) struct RawFetchResponse {
    pub(super) status_code: StatusCode,
    final_url: Url,
    content_type: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
    retry_after: Option<String>,
    pub(super) body: Vec<u8>,
    time_to_headers: Duration,
    body_download_time: Duration,
}

fn response_header(
    response: &reqwest::Response,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    response
        .headers()
        .get(name)?
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn is_html_content_type(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("text/html"))
}

pub(super) async fn fetch_raw(
    url: &Url,
    fetch_client: &FetchClient,
    max_body_size: usize,
) -> Result<RawFetchResponse> {
    fetch_client
        .validate_url(url)
        .with_context(|| format!("refusing to fetch URL: {url}"))?;
    let headers_start = Instant::now();
    let mut response = fetch_client
        .client
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("failed to fetch URL: {url}"))?;
    let time_to_headers = headers_start.elapsed();
    let status_code = response.status();
    let final_url = response.url().clone();
    let content_type = response_header(&response, reqwest::header::CONTENT_TYPE);
    let etag = response_header(&response, reqwest::header::ETAG);
    let last_modified = response_header(&response, reqwest::header::LAST_MODIFIED);
    let retry_after = response_header(&response, reqwest::header::RETRY_AFTER);
    let body_start = Instant::now();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("failed reading response body: {url}"))?
    {
        if chunk.len() > max_body_size.saturating_sub(body.len()) {
            anyhow::bail!(
                "response body for {url} exceeds configured limit of {max_body_size} bytes"
            );
        }
        body.extend_from_slice(&chunk);
    }
    let body_download_time = body_start.elapsed();

    Ok(RawFetchResponse {
        status_code,
        final_url,
        content_type,
        etag,
        last_modified,
        retry_after,
        body,
        time_to_headers,
        body_download_time,
    })
}

pub async fn fetch_url(url: &Url, fetch_client: &FetchClient) -> Result<FetchResult> {
    let raw = fetch_raw(url, fetch_client, fetch_client.max_body_size).await?;
    let response_size = raw.body.len();
    let (page, page_extraction_time) = if raw.status_code.is_success()
        && raw
            .content_type
            .as_deref()
            .is_some_and(is_html_content_type)
    {
        let extraction_start = Instant::now();
        let page = extract_page(&raw.body, &raw.final_url);
        (Some(page), extraction_start.elapsed())
    } else {
        (None, Duration::ZERO)
    };

    Ok(FetchResult {
        status_code: raw.status_code,
        final_url: raw.final_url,
        content_type: raw.content_type,
        response_size,
        etag: raw.etag,
        last_modified: raw.last_modified,
        retry_after: raw.retry_after,
        page,
        time_to_headers: raw.time_to_headers,
        body_download_time: raw.body_download_time,
        page_extraction_time,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{ErrorKind, Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Instant;

    use super::*;

    fn server(
        body: &'static [u8],
        content_encoding: Option<&'static str>,
    ) -> (Url, thread::JoinHandle<()>) {
        server_with_delays(body, content_encoding, Duration::ZERO, Duration::ZERO)
    }

    fn server_with_delays(
        body: &'static [u8],
        content_encoding: Option<&'static str>,
        header_delay: Duration,
        body_delay: Duration,
    ) -> (Url, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let bytes_read = stream.read(&mut request).unwrap();
            assert!(bytes_read > 0);
            let encoding = content_encoding
                .map(|value| format!("Content-Encoding: {value}\r\n"))
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
                body.len(),
                encoding
            );
            thread::sleep(header_delay);
            stream.write_all(response.as_bytes()).unwrap();
            thread::sleep(body_delay);
            stream.write_all(body).unwrap();
        });
        (
            Url::parse(&format!("http://{address}/body")).unwrap(),
            handle,
        )
    }

    fn server_with_response(
        status_line: &'static str,
        headers: &'static str,
        body: &'static [u8],
    ) -> (Url, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            assert!(stream.read(&mut request).unwrap() > 0);
            let response = format!(
                "{status_line}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        (
            Url::parse(&format!("http://{address}/body")).unwrap(),
            handle,
        )
    }

    #[tokio::test]
    async fn relative_links_resolve_against_redirect_destination() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for response in [
                "HTTP/1.1 302 Found\r\nLocation: /redirected/page\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 25\r\nConnection: close\r\n\r\n<a href=\"child\">child</a>",
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 1024];
                assert!(stream.read(&mut request).unwrap() > 0);
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let url = Url::parse(&format!("http://{address}/start")).unwrap();

        let result = fetch_url(&url, &FetchClient::new_for_tests())
            .await
            .unwrap();
        server.join().unwrap();

        assert_eq!(
            result.final_url.as_str(),
            format!("http://{address}/redirected/page")
        );
        let expected = format!("http://{address}/redirected/child");
        assert_eq!(
            result.page.as_ref().unwrap().links[0].target_url.as_str(),
            expected
        );
    }

    #[tokio::test]
    async fn cross_host_redirect_is_rejected_before_destination_request() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error)
                        if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("accept initial request: {error}"),
                }
            };
            let mut request = [0; 1024];
            assert!(stream.read(&mut request).unwrap() > 0);
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{}/destination\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                address.port()
            );
            stream.write_all(response.as_bytes()).unwrap();
            let deadline = Instant::now() + Duration::from_millis(250);
            loop {
                match listener.accept() {
                    Ok(_) => panic!("redirect destination was requested"),
                    Err(error)
                        if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) => panic!("accept redirect destination: {error}"),
                }
            }
        });
        let url = Url::parse(&format!("http://localhost:{}/start", address.port())).unwrap();
        let error = match fetch_url(&url, &FetchClient::new_for_tests()).await {
            Ok(_) => panic!("cross-host redirect was accepted"),
            Err(error) => error,
        };
        server.join().unwrap();
        assert!(
            error
                .chain()
                .any(|cause| cause.to_string().contains("redirect crosses crawler host"))
        );
    }

    #[tokio::test]
    async fn response_metadata_preserves_headers_and_decoded_size() {
        let gzip_body = b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x02\xff\x4b\x4c\xa4\x3d\x00\x00\x64\x7a\x70\xaf\x64\x00\x00\x00";
        let (url, server) = server_with_response(
            "HTTP/1.1 200 OK",
            "Content-Type: Text/HTML; charset=utf-8\r\nContent-Encoding: gzip\r\nETag: \"fixture-etag\"\r\nLast-Modified: Wed, 21 Oct 2015 07:28:00 GMT\r\nRetry-After: 42\r\n",
            gzip_body,
        );
        let result = fetch_url(&url, &FetchClient::new_for_tests())
            .await
            .unwrap();
        server.join().unwrap();

        assert_eq!(result.final_url, url);
        assert_eq!(
            result.content_type.as_deref(),
            Some("Text/HTML; charset=utf-8")
        );
        assert_eq!(result.etag.as_deref(), Some("\"fixture-etag\""));
        assert_eq!(
            result.last_modified.as_deref(),
            Some("Wed, 21 Oct 2015 07:28:00 GMT")
        );
        assert_eq!(result.retry_after.as_deref(), Some("42"));
        assert_eq!(result.response_size, 100);
    }

    #[tokio::test]
    async fn html_content_type_with_parameters_extracts_page() {
        let (url, server) = server_with_response(
            "HTTP/1.1 200 OK",
            "Content-Type:  text/HTML ; charset=utf-8\r\n",
            b"<h1>fixture</h1>",
        );
        let result = fetch_url(&url, &FetchClient::new_for_tests())
            .await
            .unwrap();
        server.join().unwrap();

        assert!(result.page.is_some());
    }

    #[tokio::test]
    async fn non_html_content_type_does_not_extract_page() {
        let (url, server) = server_with_response(
            "HTTP/1.1 200 OK",
            "Content-Type: text/plain\r\n",
            b"<h1>not html</h1>",
        );
        let result = fetch_url(&url, &FetchClient::new_for_tests())
            .await
            .unwrap();
        server.join().unwrap();

        assert!(result.page.is_none());
        assert_eq!(result.page_extraction_time, Duration::ZERO);
    }

    #[tokio::test]
    async fn non_success_html_does_not_extract_page() {
        let (url, server) = server_with_response(
            "HTTP/1.1 404 Not Found",
            "Content-Type: text/html\r\n",
            b"<h1>not found</h1>",
        );
        let result = fetch_url(&url, &FetchClient::new_for_tests())
            .await
            .unwrap();
        server.join().unwrap();

        assert!(result.page.is_none());
        assert_eq!(result.page_extraction_time, Duration::ZERO);
    }

    #[tokio::test]
    async fn absent_response_headers_are_none() {
        let (url, server) = server_with_response("HTTP/1.1 200 OK", "", b"body");
        let result = fetch_url(&url, &FetchClient::new_for_tests())
            .await
            .unwrap();
        server.join().unwrap();

        assert!(result.content_type.is_none());
        assert!(result.etag.is_none());
        assert!(result.last_modified.is_none());
        assert!(result.retry_after.is_none());
        assert!(result.page.is_none());
    }

    async fn fetch_with_limit(
        body: &'static [u8],
        content_encoding: Option<&'static str>,
        limit: usize,
    ) -> Result<()> {
        let (url, server) = server(body, content_encoding);
        let mut fetch_client = FetchClient::new_for_tests();
        fetch_client.max_body_size = limit;
        let result = fetch_url(&url, &fetch_client).await;
        server.join().unwrap();
        result.map(|_| ())
    }

    #[tokio::test]
    async fn header_and_body_delays_are_timed_separately() {
        let (url, server) = server_with_delays(
            b"<a href=\"/link\">link</a>",
            None,
            Duration::from_millis(100),
            Duration::from_millis(100),
        );
        let fetch_client = FetchClient::new_for_tests();
        let result = fetch_url(&url, &fetch_client).await.unwrap();
        server.join().unwrap();

        assert!(result.time_to_headers >= Duration::from_millis(50));
        assert!(result.body_download_time >= Duration::from_millis(50));
    }

    #[tokio::test]
    async fn exact_body_limit_is_accepted() {
        fetch_with_limit(b"12345", None, 5).await.unwrap();
    }

    #[tokio::test]
    async fn body_over_limit_is_rejected() {
        let error = fetch_with_limit(b"123456", None, 5).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("exceeds configured limit of 5 bytes")
        );
        assert!(error.to_string().contains("/body"));
    }

    #[tokio::test]
    async fn gzip_limit_uses_decompressed_size() {
        // gzip-compressed 100 'a' bytes; the wire payload is only 24 bytes.
        let gzip_body = b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x02\xff\x4b\x4c\xa4\x3d\x00\x00\x64\x7a\x70\xaf\x64\x00\x00\x00";
        let error = fetch_with_limit(gzip_body, Some("gzip"), 99)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("exceeds configured limit of 99 bytes")
        );
    }
}
