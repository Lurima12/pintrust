use std::path::PathBuf;

use url::Url;

/// Reverse-DNS application identifier. It is also used as the directory name
/// inside `$XDG_DATA_HOME` / `$XDG_CACHE_HOME`.
pub const APP_ID: &str = "com.lurim412.PintRust";

pub const APP_NAME: &str = "PintRust";

/// Resource prefix used by `build.rs` and the GTK icon theme.
pub const RESOURCE_PATH: &str = "/com/lurim412/PintRust";

pub const HOME_URL: &str = "https://www.pinterest.com/";

/// A current desktop Safari UA. Pinterest serves its modern, non-feature-gated
/// bundle to Safari, and WebKitGTK is much closer to Safari than to Chrome.
/// Override at runtime with `PINTRUST_USER_AGENT` if Pinterest changes its
/// sniffing rules.
pub const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15";

/// Pinterest's canonical and regional domains. Anything under one of these
/// suffixes stays inside the application.
const PINTEREST_DOMAINS: &[&str] = &[
    "pinterest.com",
    "pinterest.at",
    "pinterest.be",
    "pinterest.ca",
    "pinterest.ch",
    "pinterest.cl",
    "pinterest.co",
    "pinterest.co.in",
    "pinterest.co.kr",
    "pinterest.co.nz",
    "pinterest.co.uk",
    "pinterest.com.ar",
    "pinterest.com.au",
    "pinterest.com.br",
    "pinterest.com.co",
    "pinterest.com.mx",
    "pinterest.com.pe",
    "pinterest.com.sg",
    "pinterest.com.tr",
    "pinterest.com.ua",
    "pinterest.com.vn",
    "pinterest.cz",
    "pinterest.de",
    "pinterest.dk",
    "pinterest.es",
    "pinterest.fi",
    "pinterest.fr",
    "pinterest.gr",
    "pinterest.hu",
    "pinterest.ie",
    "pinterest.it",
    "pinterest.jp",
    "pinterest.mx",
    "pinterest.nl",
    "pinterest.no",
    "pinterest.ph",
    "pinterest.pl",
    "pinterest.pt",
    "pinterest.ro",
    "pinterest.ru",
    "pinterest.se",
    "pinterest.sk",
    "pin.it",
];

pub fn data_dir() -> PathBuf {
    glib::user_data_dir().join(APP_ID)
}

pub fn cache_dir() -> PathBuf {
    glib::user_cache_dir().join(APP_ID)
}

/// Returns true for `pinterest.com`, `www.pinterest.com`,
/// `assets.pinterest.co.uk`, `pin.it`, and so on.
pub fn is_pinterest_url(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();

    PINTEREST_DOMAINS
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// Authentication endpoints that must not be kicked out to the system browser,
/// otherwise "Continue with Google/Facebook/Apple" cannot complete.
pub fn is_auth_url(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let path = url.path().to_ascii_lowercase();

    match host.as_str() {
        "accounts.google.com"
        | "accounts.youtube.com"
        | "appleid.apple.com"
        | "login.microsoftonline.com"
        | "login.live.com" => true,
        "facebook.com" | "www.facebook.com" | "m.facebook.com" | "web.facebook.com" => {
            path.contains("dialog")
                || path.contains("login")
                || path.contains("oauth")
                || path.contains("checkpoint")
                || path.contains("recover")
        }
        _ => false,
    }
}

/// URLs that should be rendered by the embedded WebKit view.
pub fn is_internal_url(url: &Url) -> bool {
    match url.scheme() {
        "http" | "https" => is_pinterest_url(url) || is_auth_url(url),
        // Popup windows start at about:blank, and media blobs / data URLs are
        // created by the page itself.
        "about" | "blob" | "data" => true,
        _ => false,
    }
}

/// `mailto:`, `tel:` and ordinary third-party web links belong to the desktop.
pub fn user_agent() -> String {
    std::env::var("PINTRUST_USER_AGENT").unwrap_or_else(|_| USER_AGENT.to_string())
}
