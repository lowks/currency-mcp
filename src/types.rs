use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RateQuote {
    pub date: String,
    pub base: String,
    pub quote: String,
    pub rate: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Currency {
    pub iso_code: String,
    #[serde(default)]
    pub iso_numeric: String,
    pub name: String,
    #[serde(default)]
    pub symbol: String,
    pub start_date: String,
    pub end_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ApiErrorBody {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Conversion {
    pub date: String,
    pub from: String,
    pub to: String,
    pub amount: f64,
    pub rate: f64,
    pub converted: f64,
}

impl Conversion {
    pub fn from_quote(amount: f64, quote: &RateQuote) -> Self {
        Self {
            date: quote.date.clone(),
            from: quote.base.clone(),
            to: quote.quote.clone(),
            amount,
            rate: quote.rate,
            converted: amount * quote.rate,
        }
    }
}

/// MCP prompt arguments are often stringly typed on the wire.
pub fn deserialize_f64_from_string_or_number<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        Number(f64),
        Integer(i64),
        Str(String),
    }

    match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::Number(n) => Ok(n),
        StringOrNumber::Integer(n) => Ok(n as f64),
        StringOrNumber::Str(s) => s.parse().map_err(serde::de::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rate_quote() {
        let json = r#"{"date":"2026-08-14","base":"USD","quote":"EUR","rate":0.8661}"#;
        let quote: RateQuote = serde_json::from_str(json).unwrap();
        assert_eq!(quote.base, "USD");
        assert_eq!(quote.quote, "EUR");
        assert!((quote.rate - 0.8661).abs() < f64::EPSILON);
    }

    #[test]
    fn converts_using_quote() {
        let quote = RateQuote {
            date: "2026-08-14".into(),
            base: "USD".into(),
            quote: "EUR".into(),
            rate: 0.5,
        };
        let conversion = Conversion::from_quote(100.0, &quote);
        assert_eq!(conversion.converted, 50.0);
        assert_eq!(conversion.from, "USD");
        assert_eq!(conversion.to, "EUR");
    }

    #[test]
    fn deserializes_amount_from_string_or_number() {
        #[derive(Deserialize)]
        struct Sample {
            #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
            amount: f64,
        }

        let from_number: Sample = serde_json::from_str(r#"{"amount": 12.5}"#).unwrap();
        let from_int: Sample = serde_json::from_str(r#"{"amount": 12}"#).unwrap();
        let from_string: Sample = serde_json::from_str(r#"{"amount": "12.5"}"#).unwrap();
        assert_eq!(from_number.amount, 12.5);
        assert_eq!(from_int.amount, 12.0);
        assert_eq!(from_string.amount, 12.5);
    }
}
