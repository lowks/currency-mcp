use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use thiserror::Error;

use crate::security::{
    self, API_BASE, MAX_BODY_BYTES, RateLimiter, SecurityError, assert_allowed_url, parse_date,
    sanitize_error_message, validate_amount, validate_date_range, validate_query, validate_rate,
};
use crate::types::{ApiErrorBody, Conversion, Currency, RateQuote};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error("exchange-rate provider error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("network error talking to the exchange-rate provider")]
    Network,
    #[error("upstream response was too large")]
    ResponseTooLarge,
    #[error("refused to call an unexpected upstream URL")]
    UnsafeUrl,
}

#[derive(Clone)]
pub struct FrankfurterClient {
    http: reqwest::Client,
    limiter: Arc<RateLimiter>,
}

impl FrankfurterClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_limiter(Arc::new(RateLimiter::from_env()))
    }

    pub fn with_limiter(limiter: Arc<RateLimiter>) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(Self { http, limiter })
    }

    pub async fn list_currencies(&self, query: Option<&str>) -> Result<Vec<Currency>, ClientError> {
        let query = match query.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => Some(validate_query(value)?),
            None => None,
        };
        let mut currencies: Vec<Currency> =
            self.get_json(&format!("{API_BASE}/currencies")).await?;
        if let Some(query) = query {
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
        let code = security::normalize_code(code)?;
        self.get_json(&format!("{API_BASE}/currency/{code}")).await
    }

    pub async fn get_rate(
        &self,
        base: &str,
        quote: &str,
        date: Option<&str>,
    ) -> Result<RateQuote, ClientError> {
        let base = security::normalize_code(base)?;
        let quote = security::normalize_code(quote)?;
        let mut url = format!("{API_BASE}/rate/{base}/{quote}");
        if let Some(date) = date {
            let date = parse_date(date)?;
            url.push_str(&format!("?date={}", date.to_ymd()));
        }
        let rate: RateQuote = self.get_json(&url).await?;
        validate_rate(rate.rate)?;
        Ok(rate)
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
        let base = security::normalize_code(base)?;
        let mut url = format!("{API_BASE}/rates?base={base}");

        match (from, to) {
            (None, None) => {}
            (Some(from), Some(to)) => {
                if quotes.is_none() {
                    return Err(SecurityError::InvalidParam(
                        "quotes are required for a date range so the response stays bounded",
                    )
                    .into());
                }
                let (from, to) = validate_date_range(from, to)?;
                url.push_str(&format!("&from={}&to={}", from.to_ymd(), to.to_ymd()));
            }
            _ => {
                return Err(SecurityError::InvalidParam(
                    "from_date and to_date must be provided together",
                )
                .into());
            }
        }

        if let Some(quotes) = quotes {
            let quotes = security::normalize_quotes(quotes)?;
            url.push_str(&format!("&quotes={quotes}"));
        }
        if let Some(date) = date {
            let date = parse_date(date)?;
            url.push_str(&format!("&date={}", date.to_ymd()));
        }
        if let Some(group) = group.map(str::trim).filter(|value| !value.is_empty()) {
            if group.len() > 16 {
                return Err(SecurityError::InvalidParam("group must be 'week' or 'month'").into());
            }
            let group = group.to_ascii_lowercase();
            if group != "week" && group != "month" {
                return Err(SecurityError::InvalidParam("group must be 'week' or 'month'").into());
            }
            url.push_str(&format!("&group={group}"));
        }

        let rates: Vec<RateQuote> = self.get_json(&url).await?;
        for rate in &rates {
            validate_rate(rate.rate)?;
        }
        Ok(rates)
    }

    pub async fn convert(
        &self,
        amount: f64,
        from: &str,
        to: &str,
        date: Option<&str>,
    ) -> Result<Conversion, ClientError> {
        let amount = validate_amount(amount)?;
        let quote = self.get_rate(from, to, date).await?;
        let conversion = Conversion::from_quote(amount, &quote);
        if !conversion.converted.is_finite() {
            return Err(SecurityError::InvalidParam("conversion overflowed").into());
        }
        Ok(conversion)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, ClientError> {
        assert_allowed_url(url).map_err(|reason| {
            tracing::error!(url, reason, "blocked unexpected upstream URL");
            ClientError::UnsafeUrl
        })?;
        if let Err(retry_after_secs) = self.limiter.try_acquire() {
            return Err(SecurityError::RateLimited { retry_after_secs }.into());
        }

        tracing::debug!(url, "Frankfurter request");
        let response = self.http.get(url).send().await.map_err(|error| {
            tracing::warn!(%error, url, "upstream request failed");
            ClientError::Network
        })?;
        if let Some(length) = response.content_length()
            && length > MAX_BODY_BYTES as u64
        {
            return Err(ClientError::ResponseTooLarge);
        }

        let status = response.status();
        let body = response.bytes().await.map_err(|error| {
            tracing::warn!(%error, "failed to read upstream body");
            ClientError::Network
        })?;
        if body.len() > MAX_BODY_BYTES {
            return Err(ClientError::ResponseTooLarge);
        }

        if !status.is_success() {
            let raw = String::from_utf8_lossy(&body);
            let message = serde_json::from_str::<ApiErrorBody>(&raw)
                .map(|error| error.message)
                .unwrap_or_else(|_| {
                    status
                        .canonical_reason()
                        .unwrap_or("request failed")
                        .to_string()
                });
            return Err(ClientError::Api {
                status: status.as_u16(),
                message: sanitize_error_message(&message),
            });
        }

        serde_json::from_slice(&body).map_err(|error| {
            tracing::warn!(%error, "unexpected upstream JSON");
            ClientError::Api {
                status: StatusCode::OK.as_u16(),
                message: "unexpected response from the exchange-rate provider".into(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn convert_rejects_non_finite_amount() {
        let client = FrankfurterClient::with_limiter(Arc::new(RateLimiter::new(60, 20))).unwrap();
        let error = client
            .convert(f64::NAN, "USD", "EUR", None)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ClientError::Security(SecurityError::InvalidParam(_))
        ));
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
