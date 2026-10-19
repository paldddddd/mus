//! Update types and version comparison for the automatic updater.

use serde::{Deserialize, Serialize};

/// Update manifest and status returned by the update check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_notes: String,
    pub download_url: Option<String>,
    pub published_at: Option<String>,
    pub sha256: Option<String>,
}

/// Check if a remote version is strictly newer than the local version using SemVer rules.
pub fn is_newer_version(local: &str, remote: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|part| part.split('-').next().and_then(|s| s.parse().ok()))
            .collect()
    };

    let local_parts = parse(local);
    let remote_parts = parse(remote);

    for (r, l) in remote_parts.iter().zip(local_parts.iter()) {
        if r > l {
            return true;
        } else if r < l {
            return false;
        }
    }

    remote_parts.len() > local_parts.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("0.1.0", "0.1.1"));
        assert!(is_newer_version("0.1.0", "0.2.0"));
        assert!(is_newer_version("0.1.0", "1.0.0"));
        assert!(is_newer_version("v0.1.0", "v0.1.1"));
        assert!(is_newer_version("0.1.0", "0.1.0.1"));

        assert!(!is_newer_version("0.1.1", "0.1.0"));
        assert!(!is_newer_version("0.1.0", "0.1.0"));
        assert!(!is_newer_version("1.0.0", "0.9.9"));
    }
}
