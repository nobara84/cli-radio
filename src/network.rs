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
                Some(url.to_string())
            }
        };
        Ok(Proxy { url: proxy, bypass })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
