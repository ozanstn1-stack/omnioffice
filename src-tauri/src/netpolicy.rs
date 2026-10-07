//! Network policy for requests whose URL comes from untrusted input.
//!
//! The online revocation check follows OCSP and CRL URLs written into the
//! certificates of a PDF the user opened. Anyone can mint such a certificate,
//! so a URL like `http://169.254.169.254/` or `http://192.168.1.1/admin` is
//! as likely as a real CA. Left alone, the check would turn opening a PDF into
//! a request from the user's machine to its own network (SSRF) or into a
//! beacon that reveals where and when the file was opened.
//!
//! The policy, applied before every request and on every redirect hop:
//!
//! * `http` and `https` only, no credentials in the URL;
//! * port 80 or 443 only (CA endpoints practically never use another port, and
//!   anything else turns the check into a port scanner);
//! * a literal IP host must be a public address;
//! * a host name must resolve only to public addresses, and the connection is
//!   made through a resolver that filters again at connect time, so the answer
//!   cannot change between the check and the connection (DNS rebinding).
//!
//! "Public" excludes loopback, unspecified, private (RFC 1918), link-local
//! (including the 169.254.169.254 cloud metadata address), carrier-grade NAT
//! (100.64.0.0/10), unique-local (fc00::/7), multicast, broadcast, reserved
//! and documentation ranges, and IPv4 addresses embedded in IPv6 (mapped,
//! compatible, NAT64, 6to4) when the IPv4 address is not public.

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::Url;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;

/// The only ports a revocation request may use.
const ALLOWED_PORTS: [u16; 2] = [80, 443];

/// Which addresses a request may reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlPolicy {
    /// Public internet addresses only (the production policy).
    Public,
    /// No restriction beyond http(s): lets the tests talk to a loopback
    /// server. Not compiled into release builds.
    #[cfg(test)]
    Unrestricted,
}

/// True when `ip` is a globally routable unicast address.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    let blocked = a == 0 // "this network", includes 0.0.0.0
        || a == 10 // RFC 1918
        || a == 127 // loopback
        || (a == 100 && (64..=127).contains(&b)) // carrier-grade NAT
        || (a == 169 && b == 254) // link-local, cloud metadata
        || (a == 172 && (16..=31).contains(&b)) // RFC 1918
        || (a == 192 && b == 0 && (c == 0 || c == 2)) // IETF protocol assignments, documentation
        || (a == 192 && b == 88 && c == 99) // 6to4 relay anycast
        || (a == 192 && b == 168) // RFC 1918
        || (a == 198 && (b == 18 || b == 19)) // benchmarking
        || (a == 198 && b == 51 && c == 100) // documentation
        || (a == 203 && b == 0 && c == 113) // documentation
        || a >= 224; // multicast, reserved, broadcast
    !blocked
}

/// The IPv4 address held in the last 32 bits starting at segment `at`.
fn embedded_v4(segments: &[u16; 8], at: usize) -> Ipv4Addr {
    let (high, low) = (segments[at], segments[at + 1]);
    Ipv4Addr::new((high >> 8) as u8, high as u8, (low >> 8) as u8, low as u8)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
        return false;
    }
    let s = ip.segments();
    if s[0] & 0xfe00 == 0xfc00 || s[0] & 0xffc0 == 0xfe80 || s[0] & 0xffc0 == 0xfec0 {
        return false; // unique-local, link-local, (deprecated) site-local
    }
    if s[..6].iter().all(|segment| *segment == 0) {
        return is_public_v4(embedded_v4(&s, 6)); // IPv4-compatible ::a.b.c.d
    }
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6].iter().all(|segment| *segment == 0) {
        return is_public_v4(embedded_v4(&s, 6)); // NAT64 64:ff9b::/96
    }
    if s[0] == 0x64 && s[1] == 0xff9b && s[2] == 1 {
        return false; // local-use NAT64
    }
    if s[0] == 0x2002 {
        return is_public_v4(embedded_v4(&s, 1)); // 6to4
    }
    if s[0] == 0x2001 && (s[1] == 0 || s[1] == 0x0db8) {
        return false; // Teredo, documentation
    }
    if s[0] == 0x0100 && s[1..4].iter().all(|segment| *segment == 0) {
        return false; // discard-only 100::/64
    }
    true
}

/// Keeps the public addresses of a lookup; an error when none is left.
pub fn filter_public(addresses: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, String> {
    let total = addresses.len();
    let allowed: Vec<SocketAddr> = addresses.into_iter().filter(|address| is_public_ip(address.ip())).collect();
    if allowed.is_empty() {
        return Err(if total == 0 {
            "the host name does not resolve to any address".to_string()
        } else {
            "the host name resolves only to local or private addresses, which revocation checks never contact"
                .to_string()
        });
    }
    Ok(allowed)
}

impl UrlPolicy {
    /// The checks that need no lookup: scheme, credentials, port and literal
    /// IP addresses.
    pub fn check_url(self, url: &Url) -> Result<(), String> {
        if !matches!(url.scheme(), "http" | "https") {
            return Err(format!("only http:// and https:// URLs are used, not {}://", url.scheme()));
        }
        let Some(host) = url.host_str() else {
            return Err("the URL names no host".to_string());
        };
        #[cfg(test)]
        if self == Self::Unrestricted {
            return Ok(());
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("the URL carries credentials".to_string());
        }
        if let Some(port) = url.port() {
            if !ALLOWED_PORTS.contains(&port) {
                return Err(format!("port {port} is not used: only ports 80 and 443 are allowed"));
            }
        }
        // The URL parser has already turned every spelling of an IP address
        // (decimal, hex, octal, short forms) into the canonical literal.
        if url.domain().is_none() {
            match host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>() {
                Ok(ip) if is_public_ip(ip) => {}
                Ok(ip) => {
                    return Err(format!("{ip} is a local or private address, which revocation checks never contact"))
                }
                Err(_) => return Err(format!("{host} is not a usable host")),
            }
        }
        Ok(())
    }

    /// [`Self::check_url`] plus a lookup of a host name: every address it
    /// resolves to must be public.
    pub fn check_resolved(self, url: &Url) -> Result<(), String> {
        self.check_url(url)?;
        #[cfg(test)]
        if self == Self::Unrestricted {
            return Ok(());
        }
        let Some(name) = url.domain() else {
            return Ok(()); // a literal address was judged by check_url
        };
        let port = url.port_or_known_default().unwrap_or(80);
        let addresses: Vec<SocketAddr> =
            (name, port).to_socket_addrs().map_err(|error| format!("{name} could not be resolved: {error}"))?.collect();
        if addresses.is_empty() {
            return Err(format!("{name} does not resolve to any address"));
        }
        if let Some(blocked) = addresses.iter().find(|address| !is_public_ip(address.ip())) {
            return Err(format!(
                "{name} resolves to {}, a local or private address, which revocation checks never contact",
                blocked.ip()
            ));
        }
        Ok(())
    }

    /// A redirect policy that re-applies the whole policy to every hop.
    pub fn redirect_policy(self, max_hops: usize) -> reqwest::redirect::Policy {
        reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= max_hops {
                return attempt.error("too many redirects");
            }
            match self.check_resolved(attempt.url()) {
                Ok(()) => attempt.follow(),
                Err(reason) => attempt.error(format!("redirect refused: {reason}")),
            }
        })
    }

    /// Adds the connect-time address filter (the other half of the policy:
    /// without it a host name could resolve to something else when connecting
    /// than when it was checked).
    pub fn apply(self, builder: reqwest::blocking::ClientBuilder) -> reqwest::blocking::ClientBuilder {
        match self {
            Self::Public => builder.dns_resolver(Arc::new(PublicOnlyResolver)),
            #[cfg(test)]
            Self::Unrestricted => builder,
        }
    }
}

/// Resolves host names with the system resolver and drops every address that
/// is not public. Literal IP hosts never reach a resolver; `check_url` covers
/// them.
pub struct PublicOnlyResolver;

impl Resolve for PublicOnlyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let lookup = tokio::task::spawn_blocking(move || {
                (host.as_str(), 0u16).to_socket_addrs().map(|found| found.collect::<Vec<SocketAddr>>())
            })
            .await
            .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })?
            .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })?;
            let allowed = filter_public(lookup)
                .map_err(|reason| -> Box<dyn std::error::Error + Send + Sync> { reason.into() })?;
            Ok(Box::new(allowed.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().expect("an address")
    }

    fn url(text: &str) -> Url {
        Url::parse(text).expect("a URL")
    }

    #[test]
    fn local_private_and_special_ipv4_addresses_are_refused() {
        for text in [
            "0.0.0.0",
            "0.1.2.3",
            "10.0.0.1",
            "10.255.255.255",
            "100.64.0.1",
            "100.127.255.255",
            "127.0.0.1",
            "127.1.2.3",
            "169.254.169.254",
            "169.254.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.1",
            "192.0.2.1",
            "192.168.0.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.9",
            "224.0.0.1",
            "239.255.255.250",
            "240.0.0.1",
            "255.255.255.255",
        ] {
            assert!(!is_public_ip(ip(text)), "{text} must be refused");
        }
    }

    #[test]
    fn public_ipv4_addresses_next_to_the_special_ranges_are_allowed() {
        for text in [
            "1.1.1.1",
            "8.8.8.8",
            "9.255.255.255",
            "11.0.0.1",
            "100.63.255.255",
            "100.128.0.1",
            "126.255.255.255",
            "128.0.0.1",
            "169.253.1.1",
            "169.255.1.1",
            "172.15.255.255",
            "172.32.0.1",
            "192.0.1.1",
            "192.169.0.1",
            "198.17.0.1",
            "198.20.0.1",
            "223.255.255.255",
        ] {
            assert!(is_public_ip(ip(text)), "{text} must be allowed");
        }
    }

    #[test]
    fn local_private_and_special_ipv6_addresses_are_refused() {
        for text in [
            "::",
            "::1",
            "fe80::1",
            "febf::1",
            "fc00::1",
            "fd12:3456:789a::1",
            "fec0::1",
            "ff02::1",
            "ff00::",
            "2001:db8::1",
            "2001::1",
            "100::1",
            // IPv4 forms of the refused addresses
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:192.168.1.1",
            "::ffff:169.254.169.254",
            "::ffff:100.64.0.1",
            "::ffff:0.0.0.0",
            "::ffff:255.255.255.255",
            "::127.0.0.1",
            "::10.1.2.3",
            "64:ff9b::7f00:1",
            "64:ff9b::a9fe:a9fe",
            "64:ff9b:1::1",
            "2002:7f00:1::1",
            "2002:a9fe:a9fe::1",
        ] {
            assert!(!is_public_ip(ip(text)), "{text} must be refused");
        }
    }

    #[test]
    fn public_ipv6_addresses_are_allowed() {
        for text in [
            "2606:4700:4700::1111",
            "2a00:1450:4001:81b::200e",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
            "2002:808:808::1",
            "2001:4860:4860::8888",
        ] {
            assert!(is_public_ip(ip(text)), "{text} must be allowed");
        }
    }

    #[test]
    fn the_resolver_filter_drops_local_addresses_and_fails_when_nothing_is_left() {
        let mixed = vec![
            "127.0.0.1:0".parse().unwrap(),
            "93.184.216.34:0".parse().unwrap(),
            "[::1]:0".parse().unwrap(),
            "[2606:4700::1]:0".parse().unwrap(),
        ];
        let kept = filter_public(mixed).expect("public addresses remain");
        assert_eq!(kept, vec!["93.184.216.34:0".parse().unwrap(), "[2606:4700::1]:0".parse::<SocketAddr>().unwrap()]);
        let only_local = vec!["10.0.0.5:0".parse().unwrap(), "169.254.169.254:0".parse().unwrap()];
        assert!(filter_public(only_local).unwrap_err().contains("local or private"));
        assert!(filter_public(Vec::new()).unwrap_err().contains("does not resolve"));
    }

    #[test]
    fn urls_to_local_hosts_or_odd_ports_are_rejected_without_any_lookup() {
        for text in [
            "http://127.0.0.1/",
            "http://127.0.0.1:80/ocsp",
            "http://localhost.localdomain:99999/",
            "https://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/",
            "http://192.168.1.1:80/admin",
            "http://100.64.0.1/",
            "http://[::1]/",
            "http://[fe80::1]/",
            "http://[fd00::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://[::ffff:7f00:1]/",
            // other spellings of loopback normalise to a literal address
            "http://2130706433/",
            "http://0x7f.1/",
            "http://0177.0.0.1/",
            "http://127.1/",
            "http://0/",
            // ports other than 80 and 443
            "http://8.8.8.8:8080/",
            "https://8.8.8.8:22/",
            "http://8.8.8.8:9200/",
            // credentials, other schemes
            "http://user:pass@8.8.8.8/",
            "ldap://8.8.8.8/",
            "ftp://8.8.8.8/ca.crl",
            "file:///etc/passwd",
        ] {
            let Ok(parsed) = Url::parse(text) else {
                continue; // not even a URL (the port above is out of range)
            };
            assert!(UrlPolicy::Public.check_url(&parsed).is_err(), "{text} must be rejected");
            assert!(UrlPolicy::Public.check_resolved(&parsed).is_err(), "{text} must be rejected");
        }
    }

    #[test]
    fn public_literal_urls_on_the_web_ports_pass_the_static_checks() {
        for text in [
            "http://8.8.8.8/",
            "https://8.8.8.8/ocsp",
            "http://8.8.8.8:80/",
            "https://8.8.8.8:443/",
            "https://8.8.8.8:80/",
            "http://[2606:4700:4700::1111]/",
            "http://ocsp.example.test/",
            "https://crl.example.test:443/ca.crl",
        ] {
            assert_eq!(UrlPolicy::Public.check_url(&url(text)), Ok(()), "{text}");
        }
        // Literal addresses need no lookup at all.
        assert_eq!(UrlPolicy::Public.check_resolved(&url("http://8.8.8.8/")), Ok(()));
    }

    #[test]
    fn a_host_name_that_resolves_to_loopback_is_rejected() {
        // "localhost" resolves through the hosts file, no network involved.
        let error = UrlPolicy::Public.check_resolved(&url("http://localhost/")).unwrap_err();
        assert!(error.contains("local or private") || error.contains("could not be resolved"), "{error}");
    }

    #[test]
    fn the_unrestricted_test_policy_still_requires_http() {
        assert!(UrlPolicy::Unrestricted.check_url(&url("http://127.0.0.1:4000/")).is_ok());
        assert!(UrlPolicy::Unrestricted.check_url(&url("ftp://127.0.0.1/")).is_err());
    }
}
