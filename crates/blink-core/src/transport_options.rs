//! Transport settings and their limits. Port of `src/lib/transport-options.ts`.

use std::collections::BTreeMap;

use url::Url;

use crate::model::TransportOptions;

pub const MIB: u64 = 1024 * 1024;
pub const DOWNLOAD_LIMIT: u64 = 1024 * MIB;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransportField {
    TimeoutSeconds,
    ConnectTimeoutSeconds,
    MaxRedirects,
    InspectionLimitMiB,
}

impl TransportField {
    pub const ALL: [TransportField; 4] = [
        TransportField::TimeoutSeconds,
        TransportField::ConnectTimeoutSeconds,
        TransportField::MaxRedirects,
        TransportField::InspectionLimitMiB,
    ];

    /// The saved preference key.
    pub fn key(self) -> &'static str {
        match self {
            TransportField::TimeoutSeconds => "timeoutSeconds",
            TransportField::ConnectTimeoutSeconds => "connectTimeoutSeconds",
            TransportField::MaxRedirects => "maxRedirects",
            TransportField::InspectionLimitMiB => "inspectionLimitMiB",
        }
    }

    /// Inclusive `(min, max)`.
    pub fn range(self) -> (u64, u64) {
        match self {
            TransportField::TimeoutSeconds => (1, 600),
            TransportField::ConnectTimeoutSeconds => (1, 600),
            TransportField::MaxRedirects => (1, 20),
            TransportField::InspectionLimitMiB => (1, 16),
        }
    }

    pub fn get(self, options: &TransportOptions) -> u64 {
        match self {
            TransportField::TimeoutSeconds => options.timeout_seconds,
            TransportField::ConnectTimeoutSeconds => options.connect_timeout_seconds,
            TransportField::MaxRedirects => options.max_redirects,
            TransportField::InspectionLimitMiB => options.inspection_limit_mi_b,
        }
    }
}

pub fn default_transport_options() -> TransportOptions {
    TransportOptions::default()
}

pub type TransportFieldErrors = BTreeMap<TransportField, String>;

/// One message per invalid field. An empty map means the options are valid.
pub fn transport_field_errors(options: &TransportOptions) -> TransportFieldErrors {
    range_errors(|field| field.get(options) as f64)
}

/// The same checks on raw numbers, which may be fractional or NaN.
pub(crate) fn range_errors(value: impl Fn(TransportField) -> f64) -> TransportFieldErrors {
    let is_integer = |n: f64| n.is_finite() && n.fract() == 0.0;
    let timeout = value(TransportField::TimeoutSeconds);
    let mut errors = TransportFieldErrors::new();
    for field in TransportField::ALL {
        let (min, range_max) = field.range();
        // The connect timeout cannot be longer than the total timeout.
        let max = if field == TransportField::ConnectTimeoutSeconds && is_integer(timeout) {
            (range_max as f64).min(timeout)
        } else {
            range_max as f64
        };
        let n = value(field);
        if !is_integer(n) || n < min as f64 || n > max {
            errors.insert(field, format!("Enter a whole number from {min} to {max}."));
        }
    }
    errors
}

/// An error for a proxy URL the desktop transport cannot use, or "".
pub fn proxy_url_error(value: &str) -> String {
    let url = crate::request::js_trim(value);
    if url.is_empty() {
        return String::new();
    }
    if let Ok(parsed) = Url::parse(url)
        && ["http", "https", "socks5", "socks5h"].contains(&parsed.scheme())
        && parsed.host_str().is_some_and(|host| !host.is_empty())
    {
        return String::new();
    }
    "Enter an http, https, or socks5 proxy URL, such as http://127.0.0.1:8080.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_defaults() {
        assert_eq!(
            default_transport_options(),
            TransportOptions {
                timeout_seconds: 30,
                connect_timeout_seconds: 10,
                follow_redirects: false,
                max_redirects: 10,
                inspection_limit_mi_b: 4,
                verify_tls: true,
                proxy_url: String::new(),
                store_cookies: true,
            }
        );
        assert!(transport_field_errors(&default_transport_options()).is_empty());
    }

    #[test]
    fn rejects_values_out_of_range_decimals_and_nan() {
        let errors = range_errors(|field| match field {
            TransportField::TimeoutSeconds => 601.0,
            TransportField::ConnectTimeoutSeconds => 1.5,
            TransportField::MaxRedirects => 0.0,
            TransportField::InspectionLimitMiB => f64::NAN,
        });
        assert_eq!(
            errors,
            TransportFieldErrors::from([
                (
                    TransportField::TimeoutSeconds,
                    "Enter a whole number from 1 to 600.".into()
                ),
                (
                    TransportField::ConnectTimeoutSeconds,
                    "Enter a whole number from 1 to 600.".into()
                ),
                (
                    TransportField::MaxRedirects,
                    "Enter a whole number from 1 to 20.".into()
                ),
                (
                    TransportField::InspectionLimitMiB,
                    "Enter a whole number from 1 to 16.".into()
                ),
            ])
        );
    }

    #[test]
    fn limits_the_connect_timeout_to_the_total_timeout() {
        let errors = transport_field_errors(&TransportOptions {
            timeout_seconds: 5,
            connect_timeout_seconds: 6,
            ..default_transport_options()
        });
        assert_eq!(
            errors,
            TransportFieldErrors::from([(
                TransportField::ConnectTimeoutSeconds,
                "Enter a whole number from 1 to 5.".into()
            )])
        );
    }

    #[test]
    fn accepts_an_empty_http_https_or_socks5_proxy_url() {
        for url in ["", " ", "http://127.0.0.1:8080", "socks5h://proxy:1080"] {
            assert_eq!(proxy_url_error(url), "", "{url}");
        }
        for url in ["proxy:8080", "ftp://proxy", "http://"] {
            assert!(proxy_url_error(url).contains("proxy URL"), "{url}");
        }
    }
}
