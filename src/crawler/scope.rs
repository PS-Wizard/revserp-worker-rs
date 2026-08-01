use reqwest::Url;

pub(super) fn hosts_equivalent(left: &Url, right: &Url) -> bool {
    match (left.host_str(), right.host_str()) {
        (Some(left_host), Some(right_host)) => {
            strip_one_www(left_host).eq_ignore_ascii_case(strip_one_www(right_host))
        }
        _ => false,
    }
}

fn strip_one_www(host: &str) -> &str {
    host.get(4..)
        .filter(|_| host[..4].eq_ignore_ascii_case("www."))
        .unwrap_or(host)
}
