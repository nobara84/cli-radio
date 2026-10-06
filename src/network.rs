use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Intentionally no Debug: these fields may contain credentials.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Network {
    pub proxy: String,
    pub proxy_username: String,
    pub proxy_password: String,
    pub no_proxy: Option<String>,
}
#[derive(Clone)]
pub struct Proxy {
    pub url: Option<String>,
    pub bypass: String,
}
impl Proxy {
    // Display only a fixed status, never execution URLs or bypass values.
    pub fn status(&self) -> &'static str {
        if self.url.is_some() {
            "active (NO_PROXY may bypass)"
        } else {
            "inactive"
        }
    }
}
impl Network {
    pub fn resolve(
        &self,
        stream: &str,
        env: &HashMap<String, String>,
    ) -> Result<Proxy, &'static str> {
        let get = |lower: &str, upper: &str| env.get(lower).or_else(|| env.get(upper)).cloned();
        let bypass = self
            .no_proxy
            .clone()
            .or_else(|| get("no_proxy", "NO_PROXY"))
            .unwrap_or_default();
        let raw = if !self.proxy.is_empty() {
            Some(self.proxy.clone())
        } else if stream.starts_with("https:") {
            get("https_proxy", "HTTPS_PROXY").or_else(|| get("http_proxy", "HTTP_PROXY"))
        } else {
            get("http_proxy", "HTTP_PROXY")
        };
        let proxy = match raw.filter(|v| !v.is_empty()) {
            None => None,
            Some(raw) => {
                let mut url = url::Url::parse(&raw)
                    .map_err(|_| "Invalid proxy configuration (details hidden)")?;
                if url.scheme() != "http" || url.host_str().is_none() {
                    return Err("mpv requires an http:// proxy URL (also for HTTPS streams)");
                }
                if !self.proxy.is_empty() && !self.proxy_username.is_empty() {
                    url.set_username(&self.proxy_username)
                        .map_err(|_| "Invalid proxy username")?;
                    url.set_password(Some(&self.proxy_password))
                        .map_err(|_| "Invalid proxy credentials")?;
                }
                // URL serialization removes :80, but FFmpeg's proxy transport
                // requires an explicit port (including for HTTPS CONNECT).
                Some(format!(
                    "{}:{}{}",
                    &url[..url::Position::AfterHost],
                    url.port_or_known_default().unwrap_or(80),
                    &url[url::Position::AfterPort..],
                ))
            }
        };
        Ok(Proxy { url: proxy, bypass })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn execution_ports_authentication_and_display_are_separate() {
        for host in ["proxy.example.test", "[::1]"] {
            for port in ["", ":80", ":8080"] {
                for authenticated in [false, true] {
                    let auth = if authenticated {
                        "testuser:supersecret@"
                    } else {
                        ""
                    };
                    let raw = format!("http://{auth}{host}{port}");
                    let expected_port = if port == ":8080" { 8080 } else { 80 };
                    for configured in [false, true] {
                        let mut network = Network::default();
                        let mut env = HashMap::new();
                        if configured {
                            network.proxy = format!("http://{host}{port}");
                            if authenticated {
                                network.proxy_username = "testuser".into();
                                network.proxy_password = "supersecret".into();
                            }
                        } else {
                            env.insert("https_proxy".into(), raw.clone());
                        }
                        let proxy = network.resolve("https://radio.example.test", &env).unwrap();
                        let execution = proxy.url.as_ref().unwrap();
                        assert!(execution.contains(&format!("{host}:{expected_port}/")));
                        assert_eq!(execution.contains("testuser:supersecret@"), authenticated);
                        assert_eq!(proxy.status(), "active (NO_PROXY may bypass)");
                        assert!(!proxy.status().contains("testuser"));
                        assert!(!proxy.status().contains("supersecret"));
                    }
                }
            }
        }
    }

    #[test]
    fn case_priority_empty_values_and_bypass_are_deterministic() {
        let mut env = HashMap::from([
            ("http_proxy".into(), "http://lower.example.test:80".into()),
            ("HTTP_PROXY".into(), "http://upper.example.test:8080".into()),
            ("https_proxy".into(), "http://secure.example.test:80".into()),
            (
                "HTTPS_PROXY".into(),
                "http://ignored.example.test:8080".into(),
            ),
            ("no_proxy".into(), "localhost,127.0.0.1".into()),
            ("NO_PROXY".into(), "ignored.example.test".into()),
        ]);
        let n = Network::default();
        assert_eq!(
            n.resolve("https://radio", &env).unwrap().url.as_deref(),
            Some("http://secure.example.test:80/")
        );
        assert_eq!(
            n.resolve("http://radio", &env).unwrap().url.as_deref(),
            Some("http://lower.example.test:80/")
        );
        assert_eq!(
            n.resolve("https://radio", &env).unwrap().bypass,
            "localhost,127.0.0.1"
        );
        env.insert("https_proxy".into(), String::new());
        assert!(n.resolve("https://radio", &env).unwrap().url.is_none());
        env.remove("https_proxy");
        assert_eq!(
            n.resolve("https://radio", &env).unwrap().url.as_deref(),
            Some("http://ignored.example.test:8080/")
        );
        env.remove("HTTPS_PROXY");
        assert_eq!(
            n.resolve("https://radio", &env).unwrap().url.as_deref(),
            Some("http://lower.example.test:80/")
        );
        let explicit = Network {
            proxy: "http://config.example.test:80".into(),
            no_proxy: Some(String::new()),
            ..Default::default()
        };
        let p = explicit.resolve("https://radio", &env).unwrap();
        assert_eq!(p.url.as_deref(), Some("http://config.example.test:80/"));
        assert!(p.bypass.is_empty());
        env.clear();
        let direct = n.resolve("https://radio", &env).unwrap();
        assert_eq!(direct.status(), "inactive");
        assert!(direct.bypass.is_empty());
    }

    #[test]
    fn invalid_proxy_errors_never_include_input_credentials() {
        for raw in [
            "http://testuser:supersecret@",
            "https://testuser:supersecret@proxy.example.test:80",
        ] {
            let env = HashMap::from([("https_proxy".into(), raw.into())]);
            let error = Network::default()
                .resolve("https://radio", &env)
                .err()
                .unwrap();
            assert!(!error.contains("testuser"));
            assert!(!error.contains("supersecret"));
        }
    }

    #[test]
    fn priority_and_secrets() {
        let env = HashMap::from([
            ("HTTP_PROXY".into(), "http://upper:80".into()),
            ("http_proxy".into(), "http://lower:80".into()),
            ("HTTPS_PROXY".into(), "http://tls:80".into()),
            ("NO_PROXY".into(), "localhost,.example.org".into()),
        ]);
        let n = Network::default();
        assert!(
            n.resolve("http://a", &env)
                .unwrap()
                .url
                .unwrap()
                .contains("lower")
        );
        assert!(
            n.resolve("https://a", &env)
                .unwrap()
                .url
                .unwrap()
                .contains("tls")
        );
        let n = Network {
            proxy: "http://explicit:8080".into(),
            proxy_username: "a@b".into(),
            proxy_password: "p:/@ss".into(),
            ..Default::default()
        };
        let p = n.resolve("https://a", &env).unwrap();
        let parsed = url::Url::parse(p.url.as_ref().unwrap()).unwrap();
        assert_eq!(parsed.host_str(), Some("explicit"));
        assert!(p.url.unwrap().contains("a%40b"));
        assert_eq!(p.bypass, "localhost,.example.org");
        assert!(
            Network::default()
                .resolve("http://a", &HashMap::new())
                .unwrap()
                .url
                .is_none()
        );
        assert!(
            Network {
                proxy: "https://secret:password@host".into(),
                ..Default::default()
            }
            .resolve("http://a", &env)
            .err()
            .unwrap()
            .contains("http://")
        );
    }
}
