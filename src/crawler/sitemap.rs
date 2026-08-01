use std::{
    collections::{HashSet, VecDeque},
    io::Read,
};

use anyhow::{Result, bail};
use flate2::read::GzDecoder;
use quick_xml::{Reader, events::Event};
use reqwest::Url;

use super::{client::FetchClient, fetch::fetch_raw, scope::hosts_equivalent};

const ROBOTS_BODY_LIMIT: usize = 1024 * 1024;
const SITEMAP_BODY_LIMIT: usize = 50 * 1024 * 1024;
const MAX_SITEMAP_DOCUMENTS: usize = 100;
const MAX_SITEMAP_URLS: usize = 50_000;
const MAX_XML_DEPTH: usize = 256;
const MAX_LOCATION_LENGTH: usize = 16 * 1024;

struct ParsedSitemap {
    documents: Vec<String>,
    pages: Vec<String>,
}

pub(super) async fn discover_sitemap_urls(
    client: &FetchClient,
    root: &Url,
    limit: usize,
) -> Vec<Url> {
    if limit == 0 {
        return Vec::new();
    }

    let robots = origin_path(root, "/robots.txt");
    let conventional = origin_path(root, "/sitemap.xml");
    let mut pending = VecDeque::new();
    let mut queued_documents = HashSet::new();
    if let Ok(response) = fetch_raw(&robots, client, ROBOTS_BODY_LIMIT).await
        && response.status_code.is_success()
    {
        for candidate in robots_sitemaps(&response.body) {
            push_document_candidate(root, candidate, &mut pending, &mut queued_documents);
        }
    }
    push_document_candidate(
        root,
        conventional.as_str(),
        &mut pending,
        &mut queued_documents,
    );

    let mut visited_documents = HashSet::new();
    let mut pages = Vec::new();
    let mut seen_pages = HashSet::from([normalized_url(root.clone())]);
    let mut fetched_documents = 0;
    while fetched_documents < MAX_SITEMAP_DOCUMENTS && pages.len() < limit {
        let Some(document) = pending.pop_front() else {
            break;
        };
        if !visited_documents.insert(document.clone()) {
            continue;
        }
        fetched_documents += 1;
        let Ok(response) = fetch_raw(&document, client, SITEMAP_BODY_LIMIT).await else {
            continue;
        };
        if !response.status_code.is_success() {
            continue;
        }
        let Ok(body) = decode_raw_gzip(response.body, SITEMAP_BODY_LIMIT) else {
            continue;
        };
        let Ok(parsed) = parse_sitemap(&body, MAX_SITEMAP_URLS) else {
            continue;
        };

        for candidate in parsed.documents {
            push_document_candidate(root, &candidate, &mut pending, &mut queued_documents);
        }
        for candidate in parsed.pages {
            if pages.len() >= limit || pages.len() >= MAX_SITEMAP_URLS {
                break;
            }
            if let Some(url) = scoped_absolute_url(root, &candidate)
                && seen_pages.insert(url.clone())
            {
                pages.push(url);
            }
        }
    }
    pages
}

fn origin_path(root: &Url, path: &str) -> Url {
    let mut url = root.clone();
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);
    url
}

fn robots_sitemaps(body: &[u8]) -> impl Iterator<Item = &str> {
    std::str::from_utf8(body)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("sitemap")
                .then_some(value.trim())
                .filter(|value| !value.is_empty())
        })
}

fn push_document_candidate(
    root: &Url,
    candidate: &str,
    pending: &mut VecDeque<Url>,
    queued: &mut HashSet<Url>,
) {
    if queued.len() == MAX_SITEMAP_DOCUMENTS {
        return;
    }
    if let Some(url) = scoped_absolute_url(root, candidate)
        && queued.insert(url.clone())
    {
        pending.push_back(url);
    }
}

fn normalized_url(mut url: Url) -> Url {
    url.set_fragment(None);
    if url.path().is_empty() {
        url.set_path("/");
    }
    url
}

fn scoped_absolute_url(root: &Url, candidate: &str) -> Option<Url> {
    let url = Url::parse(candidate.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") || !hosts_equivalent(root, &url) {
        return None;
    }
    Some(normalized_url(url))
}

fn decode_raw_gzip(body: Vec<u8>, limit: usize) -> Result<Vec<u8>> {
    if body.get(..2) != Some(&[0x1f, 0x8b]) {
        return Ok(body);
    }

    let mut decoder = GzDecoder::new(body.as_slice());
    let mut decoded = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = decoder.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if count > limit.saturating_sub(decoded.len()) {
            bail!("gzip sitemap exceeds configured limit of {limit} bytes");
        }
        decoded.extend_from_slice(&buffer[..count]);
    }
    Ok(decoded)
}

fn decode_reference(reference: &str) -> Result<char> {
    match reference {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "apos" => Ok('\''),
        "quot" => Ok('"'),
        _ => {
            let value = reference
                .strip_prefix("#x")
                .or_else(|| reference.strip_prefix("#X"))
                .map(|value| u32::from_str_radix(value, 16))
                .or_else(|| reference.strip_prefix('#').map(|value| value.parse()))
                .ok_or_else(|| anyhow::anyhow!("unknown XML entity"))??;
            char::from_u32(value).ok_or_else(|| anyhow::anyhow!("invalid XML character entity"))
        }
    }
}

fn namespace_prefix(name: &[u8]) -> &[u8] {
    name.iter()
        .position(|byte| *byte == b':')
        .map(|index| &name[..index])
        .unwrap_or_default()
}

enum ElementKind {
    Url(Option<Vec<u8>>),
    Sitemap(Option<Vec<u8>>),
    Location { is_document: bool },
    Other,
}

impl ElementKind {
    fn direct_location(&self, prefix: &[u8]) -> Option<bool> {
        match self {
            Self::Url(parent_prefix) if parent_prefix.as_deref().unwrap_or_default() == prefix => {
                Some(false)
            }
            Self::Sitemap(parent_prefix)
                if parent_prefix.as_deref().unwrap_or_default() == prefix =>
            {
                Some(true)
            }
            _ => None,
        }
    }
}

fn append_location_text(location: &mut String, text: &str) -> Result<()> {
    if text.len() > MAX_LOCATION_LENGTH.saturating_sub(location.len()) {
        bail!("sitemap location is too long");
    }
    location.push_str(text);
    Ok(())
}

fn parse_sitemap(body: &[u8], max_locations: usize) -> Result<ParsedSitemap> {
    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut elements = Vec::<ElementKind>::new();
    let mut location: Option<(usize, String)> = None;
    let mut documents = Vec::new();
    let mut pages = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) => {
                let name = event.name();
                let local_name = event.local_name();
                let prefix = namespace_prefix(name.as_ref());
                let direct_location = if local_name.as_ref() == b"loc" {
                    elements
                        .last()
                        .and_then(|parent| parent.direct_location(prefix))
                } else {
                    None
                };
                let kind = match direct_location {
                    Some(is_document) => ElementKind::Location { is_document },
                    None if local_name.as_ref() == b"url" => {
                        ElementKind::Url((!prefix.is_empty()).then(|| prefix.to_vec()))
                    }
                    None if local_name.as_ref() == b"sitemap" => {
                        ElementKind::Sitemap((!prefix.is_empty()).then(|| prefix.to_vec()))
                    }
                    None => ElementKind::Other,
                };
                elements.push(kind);
                if elements.len() > MAX_XML_DEPTH {
                    bail!("sitemap XML is too deeply nested");
                }
                if direct_location.is_some() {
                    location = Some((elements.len(), String::new()));
                }
            }
            Event::Text(event) => {
                if let Some((depth, text)) = &mut location
                    && elements.len() == *depth
                {
                    append_location_text(text, &event.xml_content()?)?;
                }
            }
            Event::CData(event) => {
                if let Some((depth, text)) = &mut location
                    && elements.len() == *depth
                {
                    append_location_text(text, &event.decode()?)?;
                }
            }
            Event::GeneralRef(event) => {
                if let Some((depth, text)) = &mut location
                    && elements.len() == *depth
                {
                    let reference = decode_reference(&event.decode()?)?;
                    append_location_text(text, reference.encode_utf8(&mut [0; 4]))?;
                }
            }
            Event::End(_) => {
                let depth = elements.len();
                let Some(current) = elements.pop() else {
                    bail!("malformed sitemap XML");
                };
                if let ElementKind::Location { is_document } = current
                    && location
                        .as_ref()
                        .is_some_and(|(loc_depth, _)| *loc_depth == depth)
                    && let Some((_, text)) = location.take()
                {
                    let text = text.trim();
                    if !text.is_empty() {
                        if documents.len() + pages.len() >= max_locations {
                            bail!("sitemap contains too many locations");
                        }
                        if is_document {
                            documents.push(text.to_owned());
                        } else {
                            pages.push(text.to_owned());
                        }
                    }
                }
            }
            Event::DocType(_) => bail!("DTD is not allowed in sitemap XML"),
            Event::Eof => {
                if elements.is_empty() && location.is_none() {
                    break;
                }
                bail!("malformed sitemap XML");
            }
            _ => {}
        }
        buffer.clear();
    }

    Ok(ParsedSitemap { documents, pages })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };

    use flate2::{Compression, write::GzEncoder};

    use super::*;

    fn server(
        routes: HashMap<&'static str, &'static str>,
        expected_requests: usize,
    ) -> (Url, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded_requests = Arc::clone(&requests);
        let origin = format!("http://{address}");
        let handle = thread::spawn(move || {
            for _ in 0..expected_requests {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 2048];
                let bytes_read = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..bytes_read]);
                let path = request.split_whitespace().nth(1).unwrap().to_owned();
                recorded_requests.lock().unwrap().push(path.clone());
                let body = routes
                    .get(path.as_str())
                    .unwrap_or_else(|| panic!("unexpected route: {path}"))
                    .replace("{origin}", &origin);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (
            Url::parse(&format!("http://{address}/")).unwrap(),
            requests,
            handle,
        )
    }

    #[test]
    fn robots_directives_are_case_insensitive() {
        let directives = robots_sitemaps(
            b"SITEMAP: https://example.com/a.xml\n sitemap : https://example.com/b.xml\n",
        )
        .collect::<Vec<_>>();
        assert_eq!(
            directives,
            ["https://example.com/a.xml", "https://example.com/b.xml"]
        );
    }

    #[tokio::test]
    async fn root_sitemap_entry_does_not_consume_page_limit() {
        let routes = HashMap::from([
            ("/robots.txt", ""),
            (
                "/sitemap.xml",
                "<urlset><url><loc>{origin}/</loc></url><url><loc>{origin}/a</loc></url><url><loc>{origin}/b</loc></url></urlset>",
            ),
        ]);
        let (root, requests, server) = server(routes, 2);

        let pages = discover_sitemap_urls(&FetchClient::new_for_tests(), &root, 2).await;
        server.join().unwrap();

        assert_eq!(*requests.lock().unwrap(), ["/robots.txt", "/sitemap.xml"]);
        assert_eq!(pages, [root.join("/a").unwrap(), root.join("/b").unwrap()]);
    }

    #[test]
    fn document_candidates_are_capped_with_room_for_conventional_fallback() {
        let root = Url::parse("https://example.com/").unwrap();
        let mut pending = VecDeque::new();
        let mut queued = HashSet::new();
        for index in 0..MAX_SITEMAP_DOCUMENTS - 1 {
            push_document_candidate(
                &root,
                &format!("https://example.com/{index}.xml"),
                &mut pending,
                &mut queued,
            );
        }
        push_document_candidate(
            &root,
            "https://example.com/sitemap.xml",
            &mut pending,
            &mut queued,
        );
        push_document_candidate(
            &root,
            "https://example.com/ignored.xml",
            &mut pending,
            &mut queued,
        );

        assert_eq!(queued.len(), MAX_SITEMAP_DOCUMENTS);
        assert_eq!(pending.back().unwrap().path(), "/sitemap.xml");
    }

    #[test]
    fn parses_namespaced_direct_locations_with_entities_and_cdata() {
        let parsed = parse_sitemap(
            br#"<urlset xmlns="x" xmlns:image="y" xmlns:sm="x"><url><loc>https://example.com/a?x=1&amp;y=2</loc><image:loc>https://example.com/image</image:loc></url><sm:url><sm:loc><![CDATA[https://example.com/b]]></sm:loc></sm:url></urlset>"#,
            10,
        )
        .unwrap();
        assert_eq!(
            parsed.pages,
            ["https://example.com/a?x=1&y=2", "https://example.com/b"]
        );
    }

    #[tokio::test]
    async fn robots_directive_and_conventional_sitemap_are_deduplicated() {
        let routes = HashMap::from([
            ("/robots.txt", "sItEmAp: {origin}/sitemap.xml"),
            (
                "/sitemap.xml",
                "<urlset><url><loc>{origin}/page</loc></url></urlset>",
            ),
        ]);
        let (root, requests, server) = server(routes, 2);

        let pages = discover_sitemap_urls(&FetchClient::new_for_tests(), &root, 10).await;
        server.join().unwrap();

        assert_eq!(*requests.lock().unwrap(), ["/robots.txt", "/sitemap.xml"]);
        assert_eq!(pages, [root.join("/page").unwrap()]);
    }

    #[tokio::test]
    async fn sitemap_indexes_recurse_without_refetching_cycles() {
        let routes = HashMap::from([
            ("/robots.txt", ""),
            (
                "/sitemap.xml",
                "<sitemapindex><sitemap><loc>{origin}/one.xml</loc></sitemap><sitemap><loc>{origin}/two.xml</loc></sitemap></sitemapindex>",
            ),
            (
                "/one.xml",
                "<sitemapindex><sitemap><loc>{origin}/sitemap.xml</loc></sitemap><sitemap><loc>{origin}/two.xml</loc></sitemap></sitemapindex>",
            ),
            (
                "/two.xml",
                "<urlset><url><loc>{origin}/a</loc></url><url><loc>{origin}/a#duplicate</loc></url><url><loc>{origin}/b</loc></url></urlset>",
            ),
        ]);
        let (root, requests, server) = server(routes, 4);

        let pages = discover_sitemap_urls(&FetchClient::new_for_tests(), &root, 10).await;
        server.join().unwrap();

        assert_eq!(
            *requests.lock().unwrap(),
            ["/robots.txt", "/sitemap.xml", "/one.xml", "/two.xml"]
        );
        assert_eq!(pages, [root.join("/a").unwrap(), root.join("/b").unwrap()]);
    }

    #[tokio::test]
    async fn out_of_scope_sitemap_documents_are_not_fetched() {
        let routes = HashMap::from([
            ("/robots.txt", ""),
            (
                "/sitemap.xml",
                "<sitemapindex><sitemap><loc>https://external.example/a.xml</loc></sitemap><sitemap><loc>https://sub.example.com/b.xml</loc></sitemap><sitemap><loc>ftp://example.com/c.xml</loc></sitemap><sitemap><loc>/relative.xml</loc></sitemap></sitemapindex>",
            ),
        ]);
        let (root, requests, server) = server(routes, 2);

        assert!(
            discover_sitemap_urls(&FetchClient::new_for_tests(), &root, 10)
                .await
                .is_empty()
        );
        server.join().unwrap();
        assert_eq!(*requests.lock().unwrap(), ["/robots.txt", "/sitemap.xml"]);
    }

    #[test]
    fn malformed_or_doctype_sitemaps_are_rejected() {
        assert!(parse_sitemap(b"<urlset><url><loc>x</url></urlset>", 10).is_err());
        assert!(parse_sitemap(b"<!DOCTYPE sitemap><urlset/>", 10).is_err());
    }

    #[test]
    fn raw_gzip_is_capped() {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&[b'x'; 32]).unwrap();
        let gzip = encoder.finish().unwrap();
        assert_eq!(decode_raw_gzip(gzip.clone(), 32).unwrap(), vec![b'x'; 32]);
        assert!(decode_raw_gzip(gzip, 31).is_err());
    }

    #[test]
    fn scope_filter_rejects_external_subdomain_and_non_http_urls() {
        let root = Url::parse("https://example.com/").unwrap();
        assert!(scoped_absolute_url(&root, "https://sub.example.com/x").is_none());
        assert!(scoped_absolute_url(&root, "https://other.example/x").is_none());
        assert!(scoped_absolute_url(&root, "ftp://example.com/x").is_none());
        assert!(scoped_absolute_url(&root, "/relative").is_none());
    }
}
