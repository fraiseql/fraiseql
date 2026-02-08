//! Mapping of rich scalar types to their extended operators.
//!
//! This module defines which operators apply to which rich scalar types.
//! Used by the compiler to generate GraphQL types and the runtime to apply validators.

/// Information about an operator
#[derive(Debug, Clone)]
pub struct OperatorInfo {
    /// GraphQL field name (camelCase)
    pub graphql_name:   String,
    /// Type of the parameter(s)
    pub parameter_type: ParameterType,
    /// Human-readable description
    pub description:    String,
}

/// Type of parameters the operator accepts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterType {
    /// Single string value
    String,
    /// Array of strings
    StringArray,
    /// Single number
    Number,
    /// Min/max range
    NumberRange,
    /// Boolean value
    Boolean,
}

impl ParameterType {
    /// Get GraphQL type representation
    pub fn graphql_type(self) -> &'static str {
        match self {
            ParameterType::String => "String",
            ParameterType::StringArray => "[String!]!",
            ParameterType::Number => "Float",
            ParameterType::NumberRange => "FloatRange",
            ParameterType::Boolean => "Boolean",
        }
    }
}

/// Get operators for a rich scalar type by name
pub fn get_operators_for_type(type_name: &str) -> Option<Vec<OperatorInfo>> {
    match type_name {
        // CONTACT/COMMUNICATION TYPES
        "EmailAddress" => Some(email_address_operators()),
        "PhoneNumber" => Some(phone_number_operators()),
        "URL" => Some(url_operators()),
        "DomainName" => Some(domain_name_operators()),
        "Hostname" => Some(hostname_operators()),

        // LOCATION/ADDRESS TYPES
        "PostalCode" => Some(postal_code_operators()),
        "Latitude" => Some(latitude_operators()),
        "Longitude" => Some(longitude_operators()),
        "Coordinates" => Some(coordinates_operators()),
        "Timezone" => Some(timezone_operators()),
        "LocaleCode" => Some(locale_code_operators()),
        "LanguageCode" => Some(language_code_operators()),
        "CountryCode" => Some(country_code_operators()),

        // FINANCIAL TYPES
        "IBAN" => Some(iban_operators()),
        "CUSIP" => Some(cusip_operators()),
        "ISIN" => Some(isin_operators()),
        "SEDOL" => Some(sedol_operators()),
        "LEI" => Some(lei_operators()),
        "MIC" => Some(mic_operators()),
        "CurrencyCode" => Some(currency_code_operators()),
        "Money" => Some(money_operators()),
        "ExchangeCode" => Some(exchange_code_operators()),
        "ExchangeRate" => Some(exchange_rate_operators()),
        "StockSymbol" => Some(stock_symbol_operators()),

        // IDENTIFIERS & CONTENT
        "Slug" => Some(slug_operators()),
        "SemanticVersion" => Some(semantic_version_operators()),
        "HashSHA256" => Some(hash_sha256_operators()),
        "APIKey" => Some(api_key_operators()),

        // TRANSPORTATION & LOGISTICS
        "LicensePlate" => Some(license_plate_operators()),
        "VIN" => Some(vin_operators()),
        "TrackingNumber" => Some(tracking_number_operators()),
        "ContainerNumber" => Some(container_number_operators()),

        // NETWORK & GEOGRAPHY
        "IPAddress" => Some(ip_address_operators()),
        "IPv4" => Some(ipv4_operators()),
        "IPv6" => Some(ipv6_operators()),
        "CIDR" => Some(cidr_operators()),
        "Port" => Some(port_operators()),
        "AirportCode" => Some(airport_code_operators()),
        "PortCode" => Some(port_code_operators()),
        "FlightNumber" => Some(flight_number_operators()),

        // CONTENT TYPES
        "Markdown" => Some(markdown_operators()),
        "HTML" => Some(html_operators()),
        "MimeType" => Some(mime_type_operators()),
        "Color" => Some(color_operators()),
        "Image" => Some(image_operators()),
        "File" => Some(file_operators()),

        // RANGES & MEASUREMENTS
        "DateRange" => Some(date_range_operators()),
        "Duration" => Some(duration_operators()),
        "Percentage" => Some(percentage_operators()),

        _ => None,
    }
}

// Email operators
fn email_address_operators() -> Vec<OperatorInfo> {
    vec![
        OperatorInfo {
            graphql_name:   "domainEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Email domain equals (e.g., 'example.com')".to_string(),
        },
        OperatorInfo {
            graphql_name:   "domainIn".to_string(),
            parameter_type: ParameterType::StringArray,
            description:    "Email domain in list".to_string(),
        },
        OperatorInfo {
            graphql_name:   "domainEndswith".to_string(),
            parameter_type: ParameterType::String,
            description:    "Email domain ends with suffix (e.g., '.edu')".to_string(),
        },
        OperatorInfo {
            graphql_name:   "localPartStartswith".to_string(),
            parameter_type: ParameterType::String,
            description:    "Local part (before @) starts with prefix".to_string(),
        },
    ]
}

fn phone_number_operators() -> Vec<OperatorInfo> {
    vec![
        OperatorInfo {
            graphql_name:   "countryCodeEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Country code equals".to_string(),
        },
        OperatorInfo {
            graphql_name:   "countryCodeIn".to_string(),
            parameter_type: ParameterType::StringArray,
            description:    "Country code in list".to_string(),
        },
        OperatorInfo {
            graphql_name:   "isValid".to_string(),
            parameter_type: ParameterType::Boolean,
            description:    "Is valid E.164 format".to_string(),
        },
        OperatorInfo {
            graphql_name:   "typeEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Type equals (mobile, fixed, etc.)".to_string(),
        },
    ]
}

fn url_operators() -> Vec<OperatorInfo> {
    vec![
        OperatorInfo {
            graphql_name:   "protocolEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Protocol equals (http, https, etc.)".to_string(),
        },
        OperatorInfo {
            graphql_name:   "hostEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Host equals".to_string(),
        },
        OperatorInfo {
            graphql_name:   "pathStartswith".to_string(),
            parameter_type: ParameterType::String,
            description:    "Path starts with".to_string(),
        },
    ]
}

fn domain_name_operators() -> Vec<OperatorInfo> {
    vec![
        OperatorInfo {
            graphql_name:   "tldEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "TLD equals".to_string(),
        },
        OperatorInfo {
            graphql_name:   "tldIn".to_string(),
            parameter_type: ParameterType::StringArray,
            description:    "TLD in list".to_string(),
        },
    ]
}

fn hostname_operators() -> Vec<OperatorInfo> {
    vec![
        OperatorInfo {
            graphql_name:   "isFqdn".to_string(),
            parameter_type: ParameterType::Boolean,
            description:    "Is fully qualified domain name".to_string(),
        },
        OperatorInfo {
            graphql_name:   "depthEq".to_string(),
            parameter_type: ParameterType::Number,
            description:    "Label depth equals".to_string(),
        },
    ]
}

// Location operators
fn postal_code_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "countryEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Country code equals".to_string(),
    }]
}

fn latitude_operators() -> Vec<OperatorInfo> {
    vec![
        OperatorInfo {
            graphql_name:   "withinRange".to_string(),
            parameter_type: ParameterType::NumberRange,
            description:    "Latitude within range (degrees)".to_string(),
        },
        OperatorInfo {
            graphql_name:   "hemisphereEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Hemisphere equals (North/South)".to_string(),
        },
    ]
}

fn longitude_operators() -> Vec<OperatorInfo> {
    vec![
        OperatorInfo {
            graphql_name:   "withinRange".to_string(),
            parameter_type: ParameterType::NumberRange,
            description:    "Longitude within range (degrees)".to_string(),
        },
        OperatorInfo {
            graphql_name:   "hemisphereEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Hemisphere equals (East/West)".to_string(),
        },
    ]
}

fn coordinates_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "distanceWithin".to_string(),
        parameter_type: ParameterType::NumberRange,
        description:    "Distance within radius (km)".to_string(),
    }]
}

fn timezone_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Timezone equals".to_string(),
    }]
}

fn locale_code_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Locale code equals".to_string(),
    }]
}

fn language_code_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Language code equals".to_string(),
    }]
}

fn country_code_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Country code equals".to_string(),
    }]
}

// Financial operators
fn iban_operators() -> Vec<OperatorInfo> {
    vec![
        OperatorInfo {
            graphql_name:   "countryEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Country equals".to_string(),
        },
        OperatorInfo {
            graphql_name:   "checkDigitEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Check digit equals".to_string(),
        },
    ]
}

fn cusip_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "issuerEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Issuer code equals".to_string(),
    }]
}

fn isin_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "countryEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Country equals".to_string(),
    }]
}

fn sedol_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "checkDigitEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Check digit equals".to_string(),
    }]
}

fn lei_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "countryEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Country equals".to_string(),
    }]
}

fn mic_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "countryEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Country equals".to_string(),
    }]
}

fn currency_code_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Currency code equals".to_string(),
    }]
}

fn money_operators() -> Vec<OperatorInfo> {
    vec![
        OperatorInfo {
            graphql_name:   "currencyEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Currency equals".to_string(),
        },
        OperatorInfo {
            graphql_name:   "amountWithinRange".to_string(),
            parameter_type: ParameterType::NumberRange,
            description:    "Amount within range".to_string(),
        },
    ]
}

fn exchange_code_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Exchange code equals".to_string(),
    }]
}

fn exchange_rate_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "withinRange".to_string(),
        parameter_type: ParameterType::NumberRange,
        description:    "Exchange rate within range".to_string(),
    }]
}

fn stock_symbol_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Stock symbol equals".to_string(),
    }]
}

// Identifier operators
fn slug_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Slug equals".to_string(),
    }]
}

fn semantic_version_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Semantic version equals".to_string(),
    }]
}

fn hash_sha256_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Hash equals".to_string(),
    }]
}

fn api_key_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "API key equals".to_string(),
    }]
}

// Transportation operators
fn license_plate_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "countryEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Country equals".to_string(),
    }]
}

fn vin_operators() -> Vec<OperatorInfo> {
    vec![
        OperatorInfo {
            graphql_name:   "wmiEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "World Manufacturer Identifier equals".to_string(),
        },
        OperatorInfo {
            graphql_name:   "manufacturerEq".to_string(),
            parameter_type: ParameterType::String,
            description:    "Manufacturer code equals".to_string(),
        },
    ]
}

fn tracking_number_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "carrierEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Carrier equals".to_string(),
    }]
}

fn container_number_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "ownerCodeEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Owner code equals".to_string(),
    }]
}

// Network operators
fn ip_address_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "inSubnet".to_string(),
        parameter_type: ParameterType::String,
        description:    "In subnet".to_string(),
    }]
}

fn ipv4_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "IPv4 equals".to_string(),
    }]
}

fn ipv6_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "IPv6 equals".to_string(),
    }]
}

fn cidr_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "CIDR equals".to_string(),
    }]
}

fn port_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::Number,
        description:    "Port number equals".to_string(),
    }]
}

fn airport_code_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Airport code equals".to_string(),
    }]
}

fn port_code_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Port code equals".to_string(),
    }]
}

fn flight_number_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "airlineCodeEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Airline code equals".to_string(),
    }]
}

// Content operators
fn markdown_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "validFormat".to_string(),
        parameter_type: ParameterType::Boolean,
        description:    "Is valid Markdown format".to_string(),
    }]
}

fn html_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "validXml".to_string(),
        parameter_type: ParameterType::Boolean,
        description:    "Is valid HTML/XML".to_string(),
    }]
}

fn mime_type_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "eq".to_string(),
        parameter_type: ParameterType::String,
        description:    "MIME type equals".to_string(),
    }]
}

fn color_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "formatEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Color format equals".to_string(),
    }]
}

fn image_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "formatEq".to_string(),
        parameter_type: ParameterType::String,
        description:    "Image format equals".to_string(),
    }]
}

fn file_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "sizeWithinRange".to_string(),
        parameter_type: ParameterType::NumberRange,
        description:    "File size within range (bytes)".to_string(),
    }]
}

// Range operators
fn date_range_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "overlaps".to_string(),
        parameter_type: ParameterType::String,
        description:    "Date range overlaps".to_string(),
    }]
}

fn duration_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "withinRange".to_string(),
        parameter_type: ParameterType::NumberRange,
        description:    "Duration within range (seconds)".to_string(),
    }]
}

fn percentage_operators() -> Vec<OperatorInfo> {
    vec![OperatorInfo {
        graphql_name:   "withinRange".to_string(),
        parameter_type: ParameterType::NumberRange,
        description:    "Percentage within range (0-100)".to_string(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_operators() {
        let ops = get_operators_for_type("EmailAddress").unwrap();
        assert_eq!(ops.len(), 4);
        assert!(ops.iter().any(|o| o.graphql_name == "domainEq"));
    }

    #[test]
    fn test_vin_operators() {
        let ops = get_operators_for_type("VIN").unwrap();
        assert_eq!(ops.len(), 2);
        assert!(ops.iter().any(|o| o.graphql_name == "wmiEq"));
    }

    #[test]
    fn test_unknown_type() {
        assert!(get_operators_for_type("UnknownType").is_none());
    }
}
