use std::sync::Mutex;
use std::time::Instant;

use thiserror::Error;

pub const ALLOWED_HOST: &str = "api.frankfurter.dev";
pub const API_BASE: &str = "https://api.frankfurter.dev/v2";

pub const MAX_AMOUNT: f64 = 1_000_000_000_000.0;
pub const MAX_RATE: f64 = 1_000_000_000.0;
pub const MAX_QUERY_CHARS: usize = 64;
pub const MAX_CONTEXT_CHARS: usize = 400;
pub const MAX_QUOTES: usize = 32;
pub const MAX_QUOTES_INPUT_BYTES: usize = 256;
pub const MAX_CODE_INPUT_BYTES: usize = 8;
pub const MAX_DATE_INPUT_BYTES: usize = 16;
pub const MAX_RANGE_DAYS: i64 = 366;
pub const MIN_YEAR: i32 = 1948;
pub const MAX_YEAR: i32 = 2100;
pub const MAX_BODY_BYTES: usize = 1_048_576;
pub const MAX_ERROR_CHARS: usize = 180;
pub const DEFAULT_RPM: u32 = 60;
pub const DEFAULT_BURST: u32 = 20;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SecurityError {
    #[error("invalid currency code; use a 3-letter ISO 4217 code such as USD or EUR")]
    InvalidCurrency,
    #[error("invalid date '{0}'; use a real calendar date as YYYY-MM-DD")]
    InvalidDate(String),
    #[error("{0}")]
    InvalidParam(&'static str),
    #[error("rate limit exceeded; retry in {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    pub fn to_ymd(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    pub fn rata_die(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }
}

pub fn validate_amount(amount: f64) -> Result<f64, SecurityError> {
    if !amount.is_finite() {
        return Err(SecurityError::InvalidParam(
            "amount must be a finite number",
        ));
    }
    if amount < 0.0 {
        return Err(SecurityError::InvalidParam(
            "amount must be greater than or equal to 0",
        ));
    }
    if amount > MAX_AMOUNT {
        return Err(SecurityError::InvalidParam(
            "amount exceeds the maximum of 1,000,000,000,000",
        ));
    }
    Ok(amount)
}

pub fn validate_rate(rate: f64) -> Result<f64, SecurityError> {
    if !rate.is_finite() || rate <= 0.0 || rate > MAX_RATE {
        return Err(SecurityError::InvalidParam(
            "upstream returned an unusable exchange rate",
        ));
    }
    Ok(rate)
}

pub fn normalize_code(code: &str) -> Result<String, SecurityError> {
    let code = code.trim();
    if code.len() > MAX_CODE_INPUT_BYTES {
        return Err(SecurityError::InvalidCurrency);
    }
    let code = code.to_ascii_uppercase();
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(SecurityError::InvalidCurrency);
    }
    Ok(code)
}

pub fn normalize_quotes(quotes: &str) -> Result<String, SecurityError> {
    if quotes.len() > MAX_QUOTES_INPUT_BYTES {
        return Err(SecurityError::InvalidParam(
            "quotes list is too long; pass at most 32 ISO codes",
        ));
    }
    let mut codes = Vec::new();
    for part in quotes.split(',') {
        if part.trim().is_empty() {
            continue;
        }
        let code = normalize_code(part)?;
        if !codes.contains(&code) {
            codes.push(code);
        }
        if codes.len() > MAX_QUOTES {
            return Err(SecurityError::InvalidParam(
                "quotes list is too long; pass at most 32 ISO codes",
            ));
        }
    }
    if codes.is_empty() {
        return Err(SecurityError::InvalidParam(
            "quotes must be a comma-separated list of ISO 4217 codes",
        ));
    }
    Ok(codes.join(","))
}

pub fn parse_date(date: &str) -> Result<Date, SecurityError> {
    let date = date.trim();
    if date.len() > MAX_DATE_INPUT_BYTES {
        return Err(SecurityError::InvalidDate("oversized".into()));
    }
    let bytes = date.as_bytes();
    let shape_ok = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => true,
            _ => b.is_ascii_digit(),
        });
    if !shape_ok {
        return Err(SecurityError::InvalidDate(date.to_string()));
    }
    let year: i32 = date[0..4]
        .parse()
        .map_err(|_| SecurityError::InvalidDate(date.to_string()))?;
    let month: u32 = date[5..7]
        .parse()
        .map_err(|_| SecurityError::InvalidDate(date.to_string()))?;
    let day: u32 = date[8..10]
        .parse()
        .map_err(|_| SecurityError::InvalidDate(date.to_string()))?;
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
        return Err(SecurityError::InvalidDate(date.to_string()));
    }
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return Err(SecurityError::InvalidDate(date.to_string()));
    }
    Ok(Date { year, month, day })
}

pub fn validate_date_range(from: &str, to: &str) -> Result<(Date, Date), SecurityError> {
    let from = parse_date(from)?;
    let to = parse_date(to)?;
    if to < from {
        return Err(SecurityError::InvalidParam(
            "to_date must be on or after from_date",
        ));
    }
    let span = to.rata_die() - from.rata_die();
    if span > MAX_RANGE_DAYS {
        return Err(SecurityError::InvalidParam(
            "date range cannot exceed 366 days; narrow the window or use group=month",
        ));
    }
    Ok((from, to))
}

pub fn validate_query(query: &str) -> Result<&str, SecurityError> {
    let query = query.trim();
    if query.len() > MAX_QUERY_CHARS {
        return Err(SecurityError::InvalidParam(
            "query is too long; use at most 64 characters",
        ));
    }
    if query.chars().any(|c| c.is_control()) {
        return Err(SecurityError::InvalidParam(
            "query contains control characters",
        ));
    }
    Ok(query)
}

pub fn sanitize_user_text(input: &str, max_chars: usize) -> Result<String, SecurityError> {
    if input.len() > max_chars.saturating_mul(4) {
        return Err(SecurityError::InvalidParam("text is too long"));
    }
    let mut out = String::with_capacity(input.len().min(max_chars));
    let mut chars = 0usize;
    for c in input.chars() {
        let next = if c.is_control() {
            if matches!(c, '\n' | '\r' | '\t') {
                ' '
            } else {
                return Err(SecurityError::InvalidParam(
                    "text contains control characters",
                ));
            }
        } else {
            c
        };
        chars += 1;
        if chars > max_chars {
            return Err(SecurityError::InvalidParam("text is too long"));
        }
        out.push(next);
    }
    Ok(out.split_whitespace().collect::<Vec<_>>().join(" "))
}

pub fn sanitize_error_message(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| *c == ' ' || !c.is_control())
        .take(MAX_ERROR_CHARS)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "upstream request failed".to_string()
    } else {
        cleaned.to_string()
    }
}

pub fn assert_allowed_url(url: &str) -> Result<(), &'static str> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "invalid upstream URL")?;
    if parsed.scheme() != "https" {
        return Err("refusing non-HTTPS upstream URL");
    }
    if parsed.host_str() != Some(ALLOWED_HOST) {
        return Err("refusing unexpected upstream host");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("refusing upstream URL with credentials");
    }
    if parsed.port().is_some() {
        return Err("refusing upstream URL with an explicit port");
    }
    Ok(())
}

pub struct RateLimiter {
    inner: Mutex<Bucket>,
    capacity: f64,
    refill_per_sec: f64,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    pub fn new(per_minute: u32, burst: u32) -> Self {
        let capacity = f64::from(burst.max(1));
        Self {
            inner: Mutex::new(Bucket {
                tokens: capacity,
                last: Instant::now(),
            }),
            capacity,
            refill_per_sec: f64::from(per_minute.max(1)) / 60.0,
        }
    }

    pub fn from_env() -> Self {
        Self::new(
            env_u32("CURRENCY_MCP_RPM", DEFAULT_RPM),
            env_u32("CURRENCY_MCP_BURST", DEFAULT_BURST),
        )
    }

    pub fn try_acquire(&self) -> Result<(), u64> {
        let mut bucket = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let wait = ((1.0 - bucket.tokens) / self.refill_per_sec).ceil() as u64;
            Err(wait.max(1))
        }
    }
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Civil date to days, Howard Hinnant's algorithm.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month = i64::from(month);
    let day = i64::from(day);
    let month_prime = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversized_and_malformed_codes() {
        assert_eq!(normalize_code(" usd ").unwrap(), "USD");
        assert!(normalize_code("US").is_err());
        assert!(normalize_code("USD1").is_err());
        assert!(normalize_code(&"A".repeat(10_000)).is_err());
    }

    #[test]
    fn caps_quote_lists() {
        assert_eq!(normalize_quotes("eur, gbp,EUR").unwrap(), "EUR,GBP");
        assert!(normalize_quotes("").is_err());
        let too_many = (0..33)
            .map(|i| {
                let second = char::from(b'A' + (i / 26) as u8);
                let third = char::from(b'A' + (i % 26) as u8);
                format!("A{second}{third}")
            })
            .collect::<Vec<_>>()
            .join(",");
        assert!(normalize_quotes(&too_many).is_err());
    }

    #[test]
    fn validates_real_calendar_dates() {
        assert_eq!(
            parse_date("2024-02-29").unwrap(),
            Date {
                year: 2024,
                month: 2,
                day: 29
            }
        );
        assert!(parse_date("2023-02-29").is_err());
        assert!(parse_date("2024-13-01").is_err());
        assert!(parse_date("1947-12-31").is_err());
        assert!(parse_date("2024/01/02").is_err());
    }

    #[test]
    fn rejects_wide_date_ranges() {
        validate_date_range("2024-01-01", "2024-12-31").unwrap();
        assert!(validate_date_range("2024-01-02", "2024-01-01").is_err());
        assert!(validate_date_range("2020-01-01", "2022-01-02").is_err());
    }

    #[test]
    fn bounds_amounts() {
        assert!(validate_amount(f64::NAN).is_err());
        assert!(validate_amount(f64::INFINITY).is_err());
        assert!(validate_amount(-1.0).is_err());
        assert!(validate_amount(MAX_AMOUNT + 1.0).is_err());
        assert_eq!(validate_amount(0.0).unwrap(), 0.0);
    }

    #[test]
    fn sanitizes_untrusted_prompt_text() {
        assert_eq!(
            sanitize_user_text("  10 days\nin Tokyo\t ", 400).unwrap(),
            "10 days in Tokyo"
        );
        assert!(sanitize_user_text("hello\u{0007}world", 400).is_err());
        assert!(sanitize_user_text(&"x".repeat(401), 400).is_err());
    }

    #[test]
    fn pins_upstream_urls() {
        assert!(assert_allowed_url("https://api.frankfurter.dev/v2/rates").is_ok());
        assert!(assert_allowed_url("http://api.frankfurter.dev/v2/rates").is_err());
        assert!(assert_allowed_url("https://evil.example/v2/rates").is_err());
        assert!(assert_allowed_url("https://user:pass@api.frankfurter.dev/v2/rates").is_err());
        assert!(assert_allowed_url("https://api.frankfurter.dev:8443/v2/rates").is_err());
    }

    #[test]
    fn rate_limiter_blocks_after_burst() {
        let limiter = RateLimiter::new(60, 2);
        assert!(limiter.try_acquire().is_ok());
        assert!(limiter.try_acquire().is_ok());
        assert!(limiter.try_acquire().is_err());
    }

    #[test]
    fn truncates_unsafe_error_text() {
        let message = sanitize_error_message(&format!("oops\n{}", "A".repeat(500)));
        assert!(!message.contains('\n'));
        assert!(message.chars().count() <= MAX_ERROR_CHARS);
    }
}
