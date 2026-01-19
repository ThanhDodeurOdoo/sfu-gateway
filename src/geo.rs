/// Maps ISO 3166-1 alpha-2 country codes to SFU regions.
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
}
