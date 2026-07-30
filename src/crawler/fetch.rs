use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::{StatusCode, Url};

use crate::crawler::{
    client::FetchClient,
    extract::{ParsedLink, extract_links},
};

pub struct FetchResult {
    pub status_code: StatusCode,
    pub links: Vec<ParsedLink>,
    pub time_to_headers: Duration,
    pub body_download_time: Duration,
    pub link_extraction_time: Duration,
}

pub async fn fetch_url(url: &Url, fetch_client: &FetchClient) -> Result<FetchResult> {
    let headers_start = Instant::now();
    let mut response = fetch_client
        .client
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("failed to fetch URL: {url}"))?;
    let time_to_headers = headers_start.elapsed();
    let status_code = response.status();
    let body_start = Instant::now();

    let mut body = Vec::new();
    let max_body_size = fetch_client.max_body_size;
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

    let extraction_start = Instant::now();
    let links = if status_code.is_success() {
        extract_links(&body, url)
    } else {
        Vec::new()
    };
    let link_extraction_time = extraction_start.elapsed();

    Ok(FetchResult {
        status_code,
        links,
        time_to_headers,
        body_download_time,
        link_extraction_time,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

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

    async fn fetch_with_limit(
        body: &'static [u8],
        content_encoding: Option<&'static str>,
        limit: usize,
    ) -> Result<()> {
        let (url, server) = server(body, content_encoding);
        let mut fetch_client = FetchClient::new();
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
        let fetch_client = FetchClient::new();
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
