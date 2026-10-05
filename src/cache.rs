use serde_json::Value;

/// Runtime estimates reported by mpv, independent of the readahead target.
#[derive(Default, Debug, PartialEq)]
pub struct Cache {
    pub seconds: Option<f64>,
    pub bytes: Option<u64>,
    pub paused: bool,
}
impl Cache {
    pub fn update(&mut self, data: &Value) {
        self.seconds = data["cache-duration"]
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0);
        self.bytes = data["fw-bytes"].as_u64();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn parses_runtime_estimates_and_clears_missing_or_invalid_data() {
        let mut cache = Cache::default();
        cache.update(&json!({"cache-duration": 6.7, "fw-bytes": 296000}));
        assert_eq!(cache.seconds, Some(6.7));
        assert_eq!(cache.bytes, Some(296000));
        for data in [
            Value::Null,
            json!({}),
            json!({"cache-duration": -1, "fw-bytes": -2}),
            json!({"cache-duration": "10", "fw-bytes": 2.5}),
        ] {
            cache.update(&data);
            assert_eq!(cache.seconds, None);
            assert_eq!(cache.bytes, None);
        }
        cache.update(&json!({"cache-duration": 0, "fw-bytes": 0}));
        assert_eq!(cache.seconds, Some(0.0));
        assert_eq!(cache.bytes, Some(0));
    }
}
