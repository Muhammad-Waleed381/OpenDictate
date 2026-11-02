//! Asks GitHub for the newest published release. Installers are not signed, so
//! the app sends people to the release page rather than replacing itself.

use serde::{Deserialize, Serialize};

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Muhammad-Waleed381/OpenDictate/releases/latest";
const LATEST_RELEASE_PAGE: &str = "https://github.com/Muhammad-Waleed381/OpenDictate/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
}

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
}

fn compare(current: &str, tag: &str) -> Result<UpdateInfo, String> {
    let parse = |text: &str| {
        semver::Version::parse(text.trim().trim_start_matches('v'))
            .map_err(|e| format!("unreadable version '{text}': {e}"))
    };
    let (installed, latest) = (parse(current)?, parse(tag)?);
    Ok(UpdateInfo {
        current_version: installed.to_string(),
        latest_version: latest.to_string(),
        update_available: latest > installed,
    })
}

fn fetch_latest_tag() -> Result<String, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .new_agent();
    let release: LatestRelease = agent
        .get(LATEST_RELEASE_API)
        // GitHub rejects requests without a User-Agent.
        .header("User-Agent", concat!("OpenDictate/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("could not reach GitHub: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("unexpected reply from GitHub: {e}"))?;
    Ok(release.tag_name)
}

#[tauri::command]
pub async fn check_for_update() -> Result<UpdateInfo, String> {
    let tag = tauri::async_runtime::spawn_blocking(fetch_latest_tag)
        .await
        .map_err(|e| format!("update check worker failed: {e}"))??;
    compare(CURRENT_VERSION, &tag)
}

#[tauri::command]
pub fn open_update_page() -> Result<(), String> {
    tauri_plugin_opener::open_url(LATEST_RELEASE_PAGE, None::<&str>)
        .map_err(|e| format!("failed to open browser: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs network"]
    fn github_returns_a_readable_latest_release() {
        compare(CURRENT_VERSION, &fetch_latest_tag().unwrap()).unwrap();
    }

    #[test]
    fn newer_tag_is_an_update() {
        let info = compare("0.3.3", "v0.4.0").unwrap();
        assert!(info.update_available);
        assert_eq!(info.latest_version, "0.4.0");
        assert_eq!(info.current_version, "0.3.3");
    }

    #[test]
    fn same_or_older_tag_is_not_an_update() {
        assert!(!compare("0.3.3", "v0.3.3").unwrap().update_available);
        assert!(!compare("0.3.3", "0.3.2").unwrap().update_available);
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(compare("0.9.0", "v0.10.0").unwrap().update_available);
    }

    #[test]
    fn an_unreadable_tag_is_an_error_not_an_update() {
        assert!(compare("0.3.3", "nightly").is_err());
    }
}
