use crate::config::SfuConfig;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Manages SFU instances and selects the optimal one for requests.
pub struct Balancer {
    sfus: Vec<SfuInstance>,
    /// Round-robin counter for load distribution
    counter: AtomicUsize,
}

#[derive(Debug, Clone)]
pub struct SfuInstance {
    pub address: String,
    pub region: Option<String>,
    /// JWT secret key for signing tokens to this SFU (decoded bytes)
    pub key: Vec<u8>,
}

impl From<SfuConfig> for SfuInstance {
    fn from(config: SfuConfig) -> Self {
        Self {
            address: config.address,
            region: config.region,
            key: config.key,
        }
    }
}

impl Balancer {
    pub fn new(sfu_configs: Vec<SfuConfig>) -> Self {
        let sfus = sfu_configs.into_iter().map(SfuInstance::from).collect();
        Self {
            sfus,
            counter: AtomicUsize::new(0),
        }
    }

    /// Select an SFU instance based on optional region hint.
    ///
    /// Strategy:
    /// 1. If `region_hint` is provided, filter to matching regions
    /// 2. Round-robin among matching (or all) instances
    ///
    /// Returns None if no SFUs are configured.
    pub fn select(&self, region_hint: Option<&str>) -> Option<&SfuInstance> {
        if self.sfus.is_empty() {
            return None;
        }

        // Filter by region if hint provided
        let candidates: Vec<_> = region_hint.map_or_else(
            || self.sfus.iter().collect(),
            |region| {
                self.sfus
                    .iter()
                    .filter(|sfu| sfu.region.as_deref() == Some(region))
                    .collect()
            },
        );

        // Fall back to all SFUs if no region match
        let candidates = if candidates.is_empty() {
            self.sfus.iter().collect()
        } else {
            candidates
        };

        // Round-robin selection
        let index = self.counter.fetch_add(1, Ordering::Relaxed) % candidates.len();
        Some(candidates[index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sfu(address: &str, region: Option<&str>, key: &[u8]) -> SfuConfig {
        SfuConfig {
            address: address.to_string(),
            region: region.map(String::from),
            key: key.to_vec(),
        }
    }

    #[test]
    fn test_round_robin_selection() {
        let balancer = Balancer::new(vec![
            make_sfu("http://sfu1:3000", None, b"key1-padded-to-32-bytes-1234567"),
            make_sfu("http://sfu2:3000", None, b"key2-padded-to-32-bytes-1234567"),
            make_sfu("http://sfu3:3000", None, b"key3-padded-to-32-bytes-1234567"),
        ]);

        let first = balancer.select(None).unwrap().address.clone();
        let second = balancer.select(None).unwrap().address.clone();
        let third = balancer.select(None).unwrap().address.clone();
        let fourth = balancer.select(None).unwrap().address.clone();

        // Should cycle through all three
        assert_ne!(first, second);
        assert_ne!(second, third);
        // Fourth should wrap around to first
        assert_eq!(first, fourth);
    }

    #[test]
    fn test_region_filtering() {
        let balancer = Balancer::new(vec![
            make_sfu(
                "http://eu1:3000",
                Some("eu-west"),
                b"key1-padded-to-32-bytes-1234567",
            ),
            make_sfu(
                "http://eu2:3000",
                Some("eu-west"),
                b"key2-padded-to-32-bytes-1234567",
            ),
            make_sfu(
                "http://us1:3000",
                Some("us-east"),
                b"key3-padded-to-32-bytes-1234567",
            ),
        ]);

        // Select from eu-west only
        let selected = balancer.select(Some("eu-west")).unwrap();
        assert!(selected.address.starts_with("http://eu"));

        let selected = balancer.select(Some("eu-west")).unwrap();
        assert!(selected.address.starts_with("http://eu"));
    }

    #[test]
    fn test_fallback_when_no_region_match() {
        let balancer = Balancer::new(vec![
            make_sfu(
                "http://eu1:3000",
                Some("eu-west"),
                b"key1-padded-to-32-bytes-1234567",
            ),
            make_sfu(
                "http://us1:3000",
                Some("us-east"),
                b"key2-padded-to-32-bytes-1234567",
            ),
        ]);

        // Request non-existent region, should fall back to any
        let selected = balancer.select(Some("asia-pacific")).unwrap();
        assert!(!selected.address.is_empty());
    }

    #[test]
    fn test_empty_balancer() {
        let balancer = Balancer::new(vec![]);
        assert!(balancer.select(None).is_none());
    }

    #[test]
    fn test_sfu_has_key() {
        let key = b"secret-key-padded-to-32-bytes12";
        let balancer = Balancer::new(vec![make_sfu("http://sfu1:3000", None, key)]);
        let selected = balancer.select(None).unwrap();
        assert_eq!(selected.key, key);
    }
}
