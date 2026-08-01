use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use reqwest::{
    Url,
    dns::{Addrs, Name, Resolve, Resolving},
};

pub(super) fn validate_url(url: &Url, allow_loopback: bool) -> anyhow::Result<()> {
    if url.host().is_none() {
        anyhow::bail!("URL has no host");
    }
    match url.scheme() {
        "http" | "https" => {}
        scheme => anyhow::bail!("unsupported URL scheme: {scheme}"),
    }
    let host = url
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| anyhow::anyhow!("URL has no host"))?;
    let host_for_ip = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);

    if let Ok(ip) = host_for_ip.parse::<IpAddr>()
        && is_disallowed_ip(ip, allow_loopback)
    {
        anyhow::bail!("URL contains a disallowed IP address: {ip}");
    }

    Ok(())
}

pub(super) struct SafeResolver {
    allow_loopback: bool,
}

impl SafeResolver {
    pub(super) fn new(allow_loopback: bool) -> Self {
        Self { allow_loopback }
    }
}

impl Resolve for SafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let allow_loopback = self.allow_loopback;

        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let addresses = filter_resolved_addrs(&host, addresses, allow_loopback)?;
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

fn filter_resolved_addrs(
    host: &str,
    addresses: Vec<SocketAddr>,
    allow_loopback: bool,
) -> io::Result<Vec<SocketAddr>> {
    if addresses.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            format!("host {host} did not resolve to any address"),
        ));
    }

    if let Some(address) = addresses
        .iter()
        .find(|address| is_disallowed_ip(address.ip(), allow_loopback))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("host {host} resolved to disallowed address {address}"),
        ));
    }

    Ok(addresses)
}

fn is_disallowed_ip(ip: IpAddr, allow_loopback: bool) -> bool {
    match ip {
        IpAddr::V4(ip) => is_disallowed_ipv4(ip, allow_loopback),
        IpAddr::V6(ip) => {
            let octets = ip.octets();
            if octets[..10].iter().all(|byte| *byte == 0) && octets[10..12] == [0xff, 0xff] {
                let mut ipv4_octets = [0; 4];
                ipv4_octets.copy_from_slice(&octets[12..]);
                is_disallowed_ipv4(Ipv4Addr::from(ipv4_octets), allow_loopback)
            } else {
                is_disallowed_ipv6(ip, allow_loopback)
            }
        }
    }
}

fn is_disallowed_ipv4(ip: Ipv4Addr, allow_loopback: bool) -> bool {
    (ip.is_loopback() && !allow_loopback)
        || ip.is_unspecified()
        || ip.is_link_local()
        || ip.is_private()
        || ip.is_multicast()
}

fn is_disallowed_ipv6(ip: Ipv6Addr, allow_loopback: bool) -> bool {
    let octets = ip.octets();
    (ip.is_loopback() && !allow_loopback)
        || ip.is_unspecified()
        || ip.is_multicast()
        || (octets[0] & 0xfe == 0xfc)
        || (octets[0] == 0xfe && octets[1] & 0xc0 == 0x80)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn url_validation_rejects_unsupported_schemes_and_missing_hosts() {
        assert!(validate_url(&url("ftp://example.com"), false).is_err());
        assert!(validate_url(&url("file:///path"), false).is_err());
        assert!(validate_url(&url("https://example.com"), false).is_ok());
    }

    #[test]
    fn url_validation_rejects_disallowed_literal_addresses() {
        for value in [
            "http://127.0.0.1",
            "http://10.0.0.1",
            "http://172.16.0.1",
            "http://192.168.0.1",
            "http://0.0.0.0",
            "http://[::1]",
            "http://[::]",
            "http://[fc00::1]",
            "http://[fe80::1]",
            "http://[ff02::1]",
            "http://[::ffff:127.0.0.1]",
        ] {
            assert!(validate_url(&url(value), false).is_err(), "{value}");
        }

        assert!(validate_url(&url("http://8.8.8.8"), false).is_ok());
        assert!(validate_url(&url("http://[2001:4860:4860::8888]"), false).is_ok());
        assert!(validate_url(&url("http://[::ffff:8.8.8.8]"), false).is_ok());
        assert!(validate_url(&url("http://127.0.0.1"), true).is_ok());
    }

    #[test]
    fn resolver_rejects_empty_or_mixed_address_answers() {
        assert!(filter_resolved_addrs("empty", Vec::new(), false).is_err());
        assert!(
            filter_resolved_addrs(
                "mixed",
                vec![
                    "8.8.8.8:0".parse().unwrap(),
                    "192.168.1.1:0".parse().unwrap(),
                ],
                false,
            )
            .is_err()
        );
        assert!(
            filter_resolved_addrs("public", vec!["8.8.8.8:0".parse().unwrap()], false,).is_ok()
        );
    }
}
