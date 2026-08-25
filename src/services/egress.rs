//! egress guard: single validation choke point for every outbound http call
//! deko makes on behalf of an action.
//!
//! agents control target urls, which makes deko an ssrf pivot by design if
//! unchecked. this module enforces:
//!   - scheme allowlist (http/https)
//!   - no userinfo tricks (`https://expected.com@internal/`)
//!   - resolved addresses must be public unicast (blocks loopback, rfc1918,
//!     link-local incl. cloud metadata 169.254.169.254, cg nat, ula,
//!     multicast, documentation ranges, v4-mapped v6 bypasses)
//!
//! known residual risk (documented, accepted for pilot): classic dns
//! rebinding toc-tou — we validate the resolution, then reqwest re-resolves.
//! full mitigation requires pinning connections to validated ips.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

/// a url that has passed static validation. construction is the gate;
/// callers must still run `assert_resolvable` before connecting.
#[derive(Debug, Clone)]
pub struct ValidatedUrl(reqwest::Url);

impl ValidatedUrl {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// validates scheme, userinfo absence, and literal-ip safety.
    /// does not touch dns (see `assert_resolvable`).
    pub fn parse(raw: &str) -> Result<Self, String> {
        let url = reqwest::Url::parse(raw).map_err(|e| format!("unparseable target url: {}", e))?;

        match url.scheme() {
            "http" | "https" => {}
            other => return Err(format!("scheme '{}' not allowed", other)),
        }

        // `https://innocent.com@10.0.0.1/` parses with host 10.0.0.1 but
        // humans reviewing audit logs see innocent.com. refuse userinfo.
        if !url.username().is_empty() || url.password().is_some() {
            return Err("userinfo in target url not allowed".into());
        }

        let host = url.host_str().ok_or("target url has no host")?;
        // url crate keeps brackets on ipv6 literals ("[::1]"); strip them
        let bare_host = host.trim_start_matches('[').trim_end_matches(']');

        // literal ips are checked immediately; hostnames go through dns
        // at connect time via `assert_resolvable`.
        if let Ok(ip) = bare_host.parse::<IpAddr>() {
            if !is_permitted_ip(ip) {
                return Err(format!("target ip {} is in a blocked range", ip));
            }
        } else if bare_host.eq_ignore_ascii_case("localhost") {
            return Err("localhost is not a permitted target".into());
        }

        Ok(Self(url))
    }

    /// resolves the hostname and rejects any address in a blocked range.
    /// must be awaited before every connection attempt (each redirect hop).
    pub async fn assert_resolvable(&self) -> Result<(), String> {
        let raw_host = self
            .0
            .host_str()
            .ok_or_else(|| "target url has no host".to_string())?
            .to_string();
        let host = raw_host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        let port = self.0.port_or_known_default().unwrap_or(80);

        // std resolver is blocking; hop off the runtime
        let addrs = tokio::task::spawn_blocking(move || {
            (host.as_str(), port).to_socket_addrs()
        })
        .await
        .map_err(|e| format!("resolve join error: {}", e))?
        .map_err(|e| format!("dns resolution failed: {}", e))?;

        let mut saw_any = false;
        for addr in addrs {
            saw_any = true;
            if !is_permitted_ip(addr.ip()) {
                return Err(format!("resolved address {} is in a blocked range", addr.ip()));
            }
        }
        if !saw_any {
            return Err("host did not resolve to any address".into());
        }
        Ok(())
    }
}

/// true only for public unicast addresses we are willing to contact.
pub fn is_permitted_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_permitted_v4(v4),
        IpAddr::V6(v6) => is_permitted_v6(v6),
    }
}

fn is_permitted_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    // v4-mapped v6 arrives here already unwrapped by std, but guard anyway
    !(o[0] == 0                                  // this network / unspecified
        || o[0] == 10                            // rfc1918
        || o[0] == 127                           // loopback
        || (o[0] == 169 && o[1] == 254)          // link-local + cloud metadata
        || (o[0] == 172 && (16..=31).contains(&o[1])) // rfc1918
        || (o[0] == 192 && o[1] == 168)          // rfc1918
        || (o[0] == 100 && (64..=127).contains(&o[1])) // cgnat
        || (o[0] == 192 && o[1] == 0 && o[2] == 2)     // test-net-1
        || (o[0] == 198 && (o[1], o[2]) == (51, 100))  // test-net-2
        || (o[0] == 203 && o[1] == 0 && o[2] == 113)   // test-net-3
        || o[0] >= 224)                          // multicast + reserved
}

fn is_permitted_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();

    // v4-mapped ::ffff:a.b.c.d — evaluate embedded v4
    if s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0xffff {
        let v4 = Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
        return is_permitted_v4(v4);
    }

    let is_ula = (s[0] & 0xfe00) == 0xfc00;
    let is_link_local = (s[0] & 0xffc0) == 0xfe80;
    let is_multicast = (s[0] & 0xff00) == 0xff00;

    !(ip.is_loopback()
        || ip.is_unspecified()
        || is_ula
        || is_link_local
        || is_multicast
        || (s[0] == 0x2001 && s[1] == 0x0db8)) // documentation
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err_of(raw: &str) -> String {
        ValidatedUrl::parse(raw).unwrap_err()
    }

    #[test]
    fn accepts_public_https() {
        assert!(ValidatedUrl::parse("https://api.example.com/v1/do").is_ok());
        assert!(ValidatedUrl::parse("http://example.com").is_ok());
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(err_of("file:///etc/passwd").contains("not allowed"));
        assert!(err_of("ftp://example.com").contains("not allowed"));
        assert!(err_of("gopher://example.com").contains("not allowed"));
    }

    #[test]
    fn rejects_userinfo_bypass() {
        assert!(err_of("https://safe.example.com@10.0.0.5/x").contains("userinfo"));
        assert!(err_of("https://user:pass@example.com/x").contains("userinfo"));
    }

    #[test]
    fn rejects_dangerous_literal_ips() {
        for bad in [
            "http://127.0.0.1/",
            "http://10.0.0.1/",
            "http://172.16.0.9/",
            "http://172.31.255.255/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/", // aws metadata
            "http://100.64.1.1/",                       // cgnat
            "http://0.0.0.0/",
            "http://224.0.0.1/",                        // multicast
            "http://[::1]/",
            "http://[fe80::1]/",
            "http://[fc00::1]/",
            "http://[::ffff:127.0.0.1]/",               // mapped loopback
            "http://[::ffff:10.0.0.1]/",
        ] {
            assert!(ValidatedUrl::parse(bad).is_err(), "should reject {}", bad);
        }
    }

    #[test]
    fn accepts_public_literal_ips() {
        assert!(ValidatedUrl::parse("https://1.1.1.1/").is_ok());
        assert!(ValidatedUrl::parse("http://8.8.8.8:53/").is_ok());
        assert!(ValidatedUrl::parse("http://[2606:4700:4700::1111]/").is_ok());
    }

    #[test]
    fn rejects_localhost_by_name() {
        assert!(err_of("http://localhost:8080/").contains("localhost"));
    }

    #[test]
    fn permitted_range_unit_checks() {
        assert!(!is_permitted_ip("169.254.169.254".parse().unwrap()));
        assert!(!is_permitted_ip("10.1.2.3".parse().unwrap()));
        assert!(!is_permitted_ip("::ffff:192.168.0.1".parse().unwrap()));
        assert!(is_permitted_ip("93.184.216.34".parse().unwrap()));
    }
}
