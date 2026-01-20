use std::sync::atomic::{AtomicUsize, Ordering};

use crate::config::SfuConfig;
use crate::geo::region_fallback_order;

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

    /// Get available regions from configured SFUs
    fn available_regions(&self) -> Vec<&str> {
        self.sfus
            .iter()
            .filter_map(|sfu| sfu.region.as_deref())
            .collect()
    }

    /// Filter SFUs by region
    fn sfus_in_region(&self, region: &str) -> Vec<&SfuInstance> {
        self.sfus
            .iter()
            .filter(|sfu| sfu.region.as_deref() == Some(region))
            .collect()
    }

    /// Select an SFU using round-robin from candidates
    fn round_robin_select<'a>(&self, candidates: &[&'a SfuInstance]) -> Option<&'a SfuInstance> {
        if candidates.is_empty() {
            return None;
        }
        let index = self.counter.fetch_add(1, Ordering::Relaxed) % candidates.len();
        Some(candidates[index])
    }

    /// Select an SFU instance based on optional region hint.
    ///
    /// Strategy:
    /// 1. If `region_hint` is provided, try to find SFUs in that region
    /// 2. If no SFUs in that region, try nearby regions in order of proximity
    /// 3. Fall back to round-robin among all SFUs
    pub fn select(&self, region_hint: Option<&str>) -> Option<&SfuInstance> {
        if self.sfus.is_empty() {
            return None;
        }

        let Some(preferred_region) = region_hint else {
            let all: Vec<_> = self.sfus.iter().collect();
            return self.round_robin_select(&all);
        };

        // available_regions should be build one at boot time
        // and cached for the duration of the application
        // later, when SFUs register themselves, available regions
        // should be updated at runtime, but still not recomputed on access
        let available = self.available_regions();
        let fallback_order = region_fallback_order(preferred_region);

        for candidate_region in &fallback_order {
            if available.contains(candidate_region) {
                // same as above, sfus_in_region should be a fast access data structure
                // and built at boot time, or updated when the SFUs register
                let candidates = self.sfus_in_region(candidate_region);
                if !candidates.is_empty() {
                    return self.round_robin_select(&candidates);
                }
            }
        }

        // Fallback to all SFUs if no proximity match (unknown region or no match)
        let all: Vec<_> = self.sfus.iter().collect();
        self.round_robin_select(&all)
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
    fn test_fallback_to_nearby_region() {
        let balancer = Balancer::new(vec![
            make_sfu(
                "http://eu-west1:3000",
                Some("eu-west"),
                b"key1-padded-to-32-bytes-1234567",
            ),
            make_sfu(
                "http://us-east1:3000",
                Some("us-east"),
                b"key2-padded-to-32-bytes-1234567",
            ),
        ]);

        // Request eu-north, should fall back to eu-west (closest available)
        let selected = balancer.select(Some("eu-north")).unwrap();
        assert!(
            selected.address.contains("eu-west"),
            "Expected eu-west, got {}",
            selected.address
        );
    }

    #[test]
    fn test_fallback_to_distant_region() {
        let balancer = Balancer::new(vec![make_sfu(
            "http://us-east1:3000",
            Some("us-east"),
            b"key1-padded-to-32-bytes-1234567",
        )]);

        // Request ap-northeast, should eventually fall back to us-east
        let selected = balancer.select(Some("ap-northeast")).unwrap();
        assert_eq!(selected.address, "http://us-east1:3000");
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

        // Request unknown region, should fall back to any
        let selected = balancer.select(Some("unknown-region")).unwrap();
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

    #[test]
    fn test_proximity_order_from_ap_south() {
        let balancer = Balancer::new(vec![
            make_sfu(
                "http://eu-west1:3000",
                Some("eu-west"),
                b"key1-padded-to-32-bytes-1234567",
            ),
            make_sfu(
                "http://ap-southeast1:3000",
                Some("ap-southeast"),
                b"key2-padded-to-32-bytes-1234567",
            ),
        ]);

        // ap-south should prefer ap-southeast over eu-west
        let selected = balancer.select(Some("ap-south")).unwrap();
        assert!(
            selected.address.contains("ap-southeast"),
            "Expected ap-southeast to be preferred, got {}",
            selected.address
        );
    }
}
