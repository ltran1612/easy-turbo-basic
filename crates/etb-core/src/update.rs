//! Is there a newer release?
//!
//! Only when asked. The application contacts nothing on its own: no check at
//! startup, no schedule, no telemetry. The single network request it is capable
//! of happens because someone pressed a button, and the only thing it sends is
//! the request itself.
//!
//! The answer comes from GitHub's `releases/latest` redirect rather than its
//! API. That redirect is served from the CDN, so it does not spend the API's
//! unauthenticated hourly budget, and the version is in the URL it points at —
//! no JSON to parse, and no JSON parser to depend on.

use crate::error::{EtbError, Result};
use std::time::Duration;
use ureq::ResponseExt;

/// Where releases live. One constant, because the check, the download page and
/// the help text must all name the same repository.
pub const REPO: &str = "ltran1612/easy-turbo-basic";

/// Short, because someone is watching a button and nothing else is happening.
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Nothing newer than what is running.
    UpToDate,
    /// A newer release exists.
    Available { version: String },
}

/// The page to send someone to, which is also where the installer is.
pub fn releases_page() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

/// Ask GitHub what the newest release is and compare it with `current`.
pub fn check(current: &str) -> Result<Status> {
    let latest = latest_version()?;
    Ok(if is_newer(&latest, current) {
        Status::Available { version: latest }
    } else {
        Status::UpToDate
    })
}

/// Follow the redirect and read the tag out of where it lands.
fn latest_version() -> Result<String> {
    let url = format!("https://github.com/{REPO}/releases/latest");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        // A version string and nothing else. No identifier, no machine details.
        .user_agent(concat!("easy-turbo-basic/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();

    let res = agent
        .get(&url)
        .call()
        .map_err(|e| EtbError::Other(format!("could not reach GitHub: {e}")))?;

    // After redirects this is the tag page: .../releases/tag/v1.2.3
    let landed = res.get_uri().to_string();
    tag_from_url(&landed)
        .ok_or_else(|| EtbError::Other(format!("could not read a version from {landed}")))
}

/// Pull `1.2.3` out of `https://…/releases/tag/v1.2.3`.
///
/// Separated from the request so the parsing is testable without a network.
pub fn tag_from_url(url: &str) -> Option<String> {
    let tail = url.trim_end_matches('/').rsplit('/').next()?;
    let v = tail.strip_prefix('v').unwrap_or(tail);
    // Only something that looks like a version. A redirect to the releases index
    // — which is where GitHub sends you when a project has none — must not be
    // read as a release called "latest".
    parts(v).map(|_| v.to_string())
}

/// Is `candidate` a newer `x.y.z` than `current`?
///
/// Numeric per component, so 0.10.0 is newer than 0.9.0, which a string compare
/// gets backwards. Anything that is not a plain dotted number answers `false`:
/// offering an update we cannot reason about is worse than offering none.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parts(candidate), parts(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// `1.2.3` as numbers, or `None` if it is not that shape.
fn parts(v: &str) -> Option<Vec<u64>> {
    let v = v.strip_prefix('v').unwrap_or(v);
    if v.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for piece in v.split('.') {
        out.push(piece.parse::<u64>().ok()?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_higher_version_is_newer() {
        assert!(is_newer("0.1.2", "0.1.1"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn the_same_version_is_not_an_update() {
        assert!(!is_newer("0.1.1", "0.1.1"));
        assert!(
            !is_newer("v0.1.1", "0.1.1"),
            "a leading v is not a difference"
        );
    }

    #[test]
    fn an_older_version_is_never_offered() {
        assert!(!is_newer("0.1.0", "0.1.1"));
        assert!(!is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn components_compare_as_numbers_not_as_text() {
        // The case a string comparison gets backwards, and the reason this is
        // not `candidate > current` on the strings.
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.9.0", "0.10.0"));
        assert!(is_newer("0.1.10", "0.1.9"));
    }

    #[test]
    fn nothing_we_cannot_read_is_offered_as_an_update() {
        for odd in [
            "",
            "latest",
            "0.1.1-rc1",
            "nightly",
            "v",
            "1.2.x",
            "..",
            "0.1.1 ",
        ] {
            assert!(!is_newer(odd, "0.1.1"), "{odd:?} must not count as newer");
        }
        // and an unreadable *current* version cannot make everything look new
        assert!(!is_newer("9.9.9", "not-a-version"));
    }

    #[test]
    fn the_tag_comes_out_of_the_url_it_landed_on() {
        assert_eq!(
            tag_from_url("https://github.com/o/r/releases/tag/v1.2.3").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            tag_from_url("https://github.com/o/r/releases/tag/0.1.1/").as_deref(),
            Some("0.1.1")
        );
    }

    #[test]
    fn a_redirect_that_is_not_a_release_yields_nothing() {
        // What GitHub serves when a project has no releases at all. Reading
        // "latest" as a version would offer an endless update to nowhere.
        assert_eq!(tag_from_url("https://github.com/o/r/releases"), None);
        assert_eq!(tag_from_url("https://github.com/o/r/releases/latest"), None);
    }
}

/// The one test here that needs a network, so it is not part of the suite.
///
/// Run it by hand when the check itself is in question:
/// `cargo test -p etb-core -- --ignored really_asks_github`
#[cfg(test)]
mod network {
    #[test]
    #[ignore = "needs the network"]
    fn really_asks_github() {
        let status = super::check("0.0.1").expect("the check should reach GitHub");
        match status {
            super::Status::Available { version } => {
                assert!(super::parts(&version).is_some(), "got {version:?}");
                eprintln!("latest release is {version}");
            }
            super::Status::UpToDate => panic!("0.0.1 cannot be up to date"),
        }
    }
}
