use std::time::Duration;

use reqwest::StatusCode;
use thiserror::Error;

use crate::types::{ApiErrorBody, Conversion, Currency, RateQuote};

const API_BASE: &str = "https://api.frankfurter.dev/v2";

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("invalid currency code '{0}'; use an ISO 4217 code such as USD or EUR")]
    InvalidCurrency(String),
    #[error("invalid date '{0}'; use YYYY-MM-DD")]
    InvalidDate(String),
    #[error("{0}")]
    InvalidParam(String),
    #[error("Frankfurter API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("network error talking to Frankfurter: {0}")]
    Network(#[from] reqwest::Error),
}

#[derive(Clone)]
pub struct FrankfurterClient {
    http: reqwest::Client,
}

impl FrankfurterClient {
    pub fn new() -> Result<Self, ClientError> {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(Duration::from_secs(20))
            .build()?;
        Ok(Self { http })
    }

    pub async fn list_currencies(&self, query: Option<&str>) -> Result<Vec<Currency>, ClientError> {
        let mut currencies: Vec<Currency> =
            self.get_json(&format!("{API_BASE}/currencies")).await?;
        if let Some(query) = query.map(str::trim).filter(|q| !q.is_empty()) {
            let needle = query.to_ascii_uppercase();
            currencies.retain(|currency| {
                currency.iso_code.to_ascii_uppercase().contains(&needle)
                    || currency.name.to_ascii_uppercase().contains(&needle)
            });
        }
        currencies.sort_by(|a, b| a.iso_code.cmp(&b.iso_code));
        Ok(currencies)
    }

    pub async fn get_currency(&self, code: &str) -> Result<Currency, ClientError> {
        let code = normalize_code(code)?;
        self.get_json(&format!("{API_BASE}/currency/{code}")).await
    }

    pub async fn get_rate(
        &self,
        base: &str,
        quote: &str,
        date: Option<&str>,
    ) -> Result<RateQuote, ClientError> {
        let base = normalize_code(base)?;
        let quote = normalize_code(quote)?;
        let mut url = format!("{API_BASE}/rate/{base}/{quote}");
        if let Some(date) = date {
            let date = validate_date(date)?;
            url.push_str(&format!("?date={date}"));
        }
        self.get_json(&url).await
    }

    pub async fn get_rates(
        &self,
        base: &str,
        quotes: Option<&str>,
        date: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        group: Option<&str>,
    ) -> Result<Vec<RateQuote>, ClientError> {
        let base = normalize_code(base)?;
        let mut url = format!("{API_BASE}/rates?base={base}");

        if let Some(quotes) = quotes {
            let quotes = normalize_quotes(quotes)?;
            url.push_str(&format!("&quotes={quotes}"));
        }
        if let Some(date) = date {
            let date = validate_date(date)?;
            url.push_str(&format!("&date={date}"));
        }
        if let Some(from) = from {
            let from = validate_date(from)?;
            url.push_str(&format!("&from={from}"));
        }
        if let Some(to) = to {
            let to = validate_date(to)?;
            url.push_str(&format!("&to={to}"));
        }
        if let Some(group) = group.map(str::trim).filter(|g| !g.is_empty()) {
            let group = group.to_ascii_lowercase();
            if group != "week" && group != "month" {
                return Err(ClientError::InvalidParam(
                    "group must be 'week' or 'month'".into(),
                ));
            }
            url.push_str(&format!("&group={group}"));
        }

        self.get_json(&url).await
    }

    pub async fn convert(
        &self,
        amount: f64,
        from: &str,
        to: &str,
        date: Option<&str>,
    ) -> Result<Conversion, ClientError> {
        if !amount.is_finite() || amount < 0.0 {
            return Err(ClientError::InvalidParam(
                "amount must be a finite number greater than or equal to 0".into(),
            ));
        }
        let quote = self.get_rate(from, to, date).await?;
        Ok(Conversion::from_quote(amount, &quote))
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, ClientError> {
        tracing::debug!(url, "Frankfurter request");
        let response = self.http.get(url).send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            let message = serde_json::from_str::<ApiErrorBody>(&body)
                .map(|error| error.message)
                .unwrap_or_else(|_| {
                    if body.is_empty() {
                        status
                            .canonical_reason()
                            .unwrap_or("request failed")
                            .to_string()
                    } else {
                        body
                    }
                });
            return Err(ClientError::Api {
                status: status.as_u16(),
                message,
            });
        }
        serde_json::from_str(&body).map_err(|error| ClientError::Api {
            status: StatusCode::OK.as_u16(),
            message: format!("unexpected response from Frankfurter: {error}"),
        })
    }
}

fn normalize_code(code: &str) -> Result<String, ClientError> {
    let code = code.trim().to_ascii_uppercase();
    if code.len() != 3 || !code.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(ClientError::InvalidCurrency(code));
    }
    Ok(code)
}

fn normalize_quotes(quotes: &str) -> Result<String, ClientError> {
    let mut codes = Vec::new();
    for part in quotes.split(',') {
        let code = normalize_code(part)?;
        if !codes.contains(&code) {
            codes.push(code);
        }
    }
    if codes.is_empty() {
        return Err(ClientError::InvalidParam(
            "quotes must be a comma-separated list of ISO 4217 codes".into(),
        ));
    }
    Ok(codes.join(","))
}

fn validate_date(date: &str) -> Result<&str, ClientError> {
    let date = date.trim();
    let valid = date.len() == 10
        && date.as_bytes()[4] == b'-'
        && date.as_bytes()[7] == b'-'
        && date.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => true,
            _ => b.is_ascii_digit(),
        });
    if valid {
        Ok(date)
    } else {
        Err(ClientError::InvalidDate(date.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_currency_codes() {
        assert_eq!(normalize_code(" usd ").unwrap(), "USD");
        assert!(normalize_code("US").is_err());
        assert!(normalize_code("USD1").is_err());
        assert!(normalize_code("US$").is_err());
    }

    #[test]
    fn normalizes_quote_lists() {
        assert_eq!(normalize_quotes("eur, gbp,EUR").unwrap(), "EUR,GBP");
        assert!(normalize_quotes("").is_err());
        assert!(normalize_quotes("EURO").is_err());
    }

    #[test]
    fn validates_iso_dates() {
        assert_eq!(validate_date("2024-01-02").unwrap(), "2024-01-02");
        assert!(validate_date("2024/01/02").is_err());
        assert!(validate_date("yesterday").is_err());
    }

    #[tokio::test]
    async fn live_convert_usd_to_eur() {
        let client = FrankfurterClient::new().unwrap();
        let conversion = client
            .convert(100.0, "usd", "eur", None)
            .await
            .expect("Frankfurter should return a USD/EUR rate");
        assert_eq!(conversion.from, "USD");
        assert_eq!(conversion.to, "EUR");
        assert!(conversion.rate > 0.0);
        assert!((conversion.converted - 100.0 * conversion.rate).abs() < 1e-9);
    }
}
