/// Maps ISO 3166-1 alpha-2 country codes to SFU regions.
#[must_use]
pub fn country_to_region(country_code: &str) -> Option<&'static str> {
    match country_code.to_uppercase().as_str() {
        // Western Europe
        "FR" | "DE" | "GB" | "ES" | "IT" | "NL" | "BE" | "PT" | "IE" | "AT" | "CH" | "LU"
        | "MC" | "AD" | "MT" | "SM" | "VA" | "LI" => Some("eu-west"),

        // Northern Europe
        "SE" | "NO" | "DK" | "FI" | "IS" | "EE" | "LV" | "LT" => Some("eu-north"),

        // Eastern/Central Europe + Russia + Central Asia
        "PL" | "CZ" | "SK" | "HU" | "RO" | "BG" | "HR" | "SI" | "RS" | "BA" | "ME" | "MK"
        | "AL" | "XK" | "MD" | "UA" | "BY" | "RU" | "KZ" | "UZ" | "TM" | "KG" | "TJ" | "AZ"
        | "GE" | "AM" => Some("eu-central"),

        // Greece, Turkey, Cyprus, North Africa
        "GR" | "TR" | "CY" | "EG" | "LY" | "TN" | "DZ" | "MA" => Some("eu-south"),

        // US, Canada, Mexico, Central America, Caribbean
        "US" | "CA" | "MX" | "GT" | "BZ" | "SV" | "HN" | "NI" | "CR" | "PA" | "CU" | "JM"
        | "HT" | "DO" | "PR" | "TT" | "BB" | "BS" => Some("us-east"),

        // South America - East
        "BR" | "AR" | "UY" | "PY" | "VE" | "CO" | "GY" | "SR" | "GF" => Some("sa-east"),

        // South America - West (Andes)
        "CL" | "PE" | "EC" | "BO" => Some("sa-west"),

        // East Asia
        "JP" | "KR" | "TW" | "HK" | "MO" => Some("ap-northeast"),

        // China
        "CN" => Some("ap-east"),

        // Southeast Asia + Oceania
        "SG" | "MY" | "TH" | "VN" | "ID" | "PH" | "MM" | "KH" | "LA" | "BN" | "AU" | "NZ"
        | "FJ" | "PG" | "NC" | "VU" | "WS" | "TO" => Some("ap-southeast"),

        // South Asia
        "IN" | "PK" | "BD" | "LK" | "NP" | "BT" | "MV" => Some("ap-south"),

        // Middle East
        "AE" | "SA" | "QA" | "KW" | "BH" | "OM" | "IL" | "JO" | "LB" | "IQ" | "IR" | "YE" => {
            Some("me-south")
        }

        // Africa - Sub-Saharan
        "ZA" | "NG" | "KE" | "GH" | "TZ" | "UG" | "ET" | "SN" | "CI" | "CM" | "AO" | "ZW"
        | "ZM" | "MZ" | "BW" | "NA" | "RW" | "MU" | "MG" => Some("af-south"),

        _ => None,
    }
}

/// Region with approximate geographic coordinates (latitude, longitude).
struct RegionCoord {
    name: &'static str,
    lat: f64,
    lon: f64,
}

/// All known regions with their approximate center coordinates.
const REGIONS: &[RegionCoord] = &[
    RegionCoord {
        name: "eu-west",
        lat: 48.8,
        lon: 2.3,
    }, // Paris
    RegionCoord {
        name: "eu-north",
        lat: 59.3,
        lon: 18.0,
    }, // Stockholm
    RegionCoord {
        name: "eu-central",
        lat: 52.5,
        lon: 13.4,
    }, // Berlin (TODO verify, i think frankfurt is more common for server location)
    RegionCoord {
        name: "eu-south",
        lat: 41.9,
        lon: 12.5,
    }, // Rome
    RegionCoord {
        name: "us-east",
        lat: 39.0,
        lon: -77.0,
    }, // Virginia
    RegionCoord {
        name: "sa-east",
        lat: -23.5,
        lon: -46.6,
    }, // São Paulo
    RegionCoord {
        name: "sa-west",
        lat: -33.4,
        lon: -70.6,
    }, // Santiago
    RegionCoord {
        name: "ap-northeast",
        lat: 35.7,
        lon: 139.7,
    }, // Tokyo
    RegionCoord {
        name: "ap-east",
        lat: 31.2,
        lon: 121.5,
    }, // Shanghai
    RegionCoord {
        name: "ap-southeast",
        lat: 1.3,
        lon: 103.8,
    }, // Singapore
    RegionCoord {
        name: "ap-south",
        lat: 19.0,
        lon: 72.8,
    }, // Mumbai
    RegionCoord {
        name: "me-south",
        lat: 25.3,
        lon: 55.3,
    }, // Dubai
    RegionCoord {
        name: "af-south",
        lat: -26.2,
        lon: 28.0,
    }, // Johannesburg
];

/// Get coordinates for a region, returns None if unknown.
fn region_coords(region: &str) -> Option<(f64, f64)> {
    REGIONS
        .iter()
        .find(|r| r.name == region)
        .map(|r| (r.lat, r.lon))
}

/// Approximate great-circle distance using Haversine formula (returns km).
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;

    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();

    let a =
        (d_lat / 2.0).sin().powi(2) + lat1_rad.cos() * lat2_rad.cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_KM * c
}

/// Returns regions ordered by proximity from the given region.
/// Unknown regions return an empty vector.
#[must_use]
pub fn region_fallback_order(region: &str) -> Vec<&'static str> {
    let Some((origin_lat, origin_lon)) = region_coords(region) else {
        return Vec::new();
    };

    let mut regions_with_distance: Vec<_> = REGIONS
        .iter()
        .map(|r| {
            let dist = haversine_distance(origin_lat, origin_lon, r.lat, r.lon);
            (r.name, dist)
        })
        .collect();

    regions_with_distance
        .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    regions_with_distance
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_european_countries() {
        assert_eq!(country_to_region("FR"), Some("eu-west"));
        assert_eq!(country_to_region("DE"), Some("eu-west"));
        assert_eq!(country_to_region("GB"), Some("eu-west"));
        assert_eq!(country_to_region("SE"), Some("eu-north"));
        assert_eq!(country_to_region("PL"), Some("eu-central"));
    }

    #[test]
    fn test_north_america() {
        assert_eq!(country_to_region("US"), Some("us-east"));
        assert_eq!(country_to_region("CA"), Some("us-east"));
        assert_eq!(country_to_region("MX"), Some("us-east"));
    }

    #[test]
    fn test_asia_pacific() {
        assert_eq!(country_to_region("JP"), Some("ap-northeast"));
        assert_eq!(country_to_region("AU"), Some("ap-southeast"));
        assert_eq!(country_to_region("IN"), Some("ap-south"));
        assert_eq!(country_to_region("SG"), Some("ap-southeast"));
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(country_to_region("fr"), Some("eu-west"));
        assert_eq!(country_to_region("Fr"), Some("eu-west"));
        assert_eq!(country_to_region("FR"), Some("eu-west"));
    }

    #[test]
    fn test_unknown_country_returns_none() {
        assert_eq!(country_to_region("XX"), None);
        assert_eq!(country_to_region("ZZ"), None);
        assert_eq!(country_to_region(""), None);
    }

    #[test]
    fn test_south_america() {
        assert_eq!(country_to_region("BR"), Some("sa-east"));
        assert_eq!(country_to_region("AR"), Some("sa-east"));
        assert_eq!(country_to_region("CL"), Some("sa-west"));
    }

    #[test]
    fn test_middle_east() {
        assert_eq!(country_to_region("AE"), Some("me-south"));
        assert_eq!(country_to_region("SA"), Some("me-south"));
    }

    #[test]
    fn test_africa() {
        assert_eq!(country_to_region("ZA"), Some("af-south"));
        assert_eq!(country_to_region("EG"), Some("eu-south"));
    }

    #[test]
    fn test_region_fallback_starts_with_self() {
        assert_eq!(region_fallback_order("eu-west")[0], "eu-west");
        assert_eq!(region_fallback_order("us-east")[0], "us-east");
        assert_eq!(region_fallback_order("ap-northeast")[0], "ap-northeast");
    }

    #[test]
    fn test_region_fallback_eu_west_prefers_nearby() {
        let order = region_fallback_order("eu-west");
        // eu-central and eu-north should be in the top 3 (after eu-west itself)
        let top3 = &order[0..4];
        assert!(top3.contains(&"eu-central"));
        assert!(top3.contains(&"eu-north"));
    }

    #[test]
    fn test_region_fallback_unknown_returns_empty() {
        assert!(region_fallback_order("unknown-region").is_empty());
        assert!(region_fallback_order("").is_empty());
    }

    #[test]
    fn test_region_fallback_all_regions_covered() {
        let order = region_fallback_order("eu-west");
        assert_eq!(order.len(), 13);
    }

    #[test]
    fn test_ap_south_prefers_ap_southeast() {
        let order = region_fallback_order("ap-south");
        let ap_southeast_idx = order.iter().position(|&r| r == "ap-southeast");
        let eu_west_idx = order.iter().position(|&r| r == "eu-west");
        assert!(
            ap_southeast_idx < eu_west_idx,
            "ap-southeast should be closer to ap-south than eu-west"
        );
    }

    #[test]
    fn test_haversine_distance() {
        let (paris_lat, paris_lon) = region_coords("eu-west").unwrap();
        let (berlin_lat, berlin_lon) = region_coords("eu-central").unwrap();
        let (singapore_lat, singapore_lon) = region_coords("ap-southeast").unwrap();

        let dist = haversine_distance(paris_lat, paris_lon, berlin_lat, berlin_lon);
        assert!(
            dist > 800.0 && dist < 1000.0,
            "Paris-Berlin distance: {dist}"
        );

        let dist = haversine_distance(paris_lat, paris_lon, singapore_lat, singapore_lon);
        assert!(
            dist > 10500.0 && dist < 11000.0,
            "Paris-Singapore distance: {dist}"
        );
    }
}
