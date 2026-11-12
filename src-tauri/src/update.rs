//! Asks GitHub for the newest published release and can install it.
//!
//! Installing picks the release asset that matches how this copy of the app
//! was installed (Windows NSIS setup, macOS app bundle, Linux AppImage, deb or
//! rpm), downloads it, checks it against the SHA-256 digest GitHub publishes
//! for the asset, then hands it to the platform's installer and restarts.
//! The installers themselves are not code-signed, so the digest only proves
//! the file arrived intact from the release; when no matching asset or
//! install method exists the app falls back to the release page.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(unix)]
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Muhammad-Waleed381/OpenDictate/releases/latest";
const LATEST_RELEASE_PAGE: &str = "https://github.com/Muhammad-Waleed381/OpenDictate/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Installers are only fetched from this repository's release downloads.
const DOWNLOAD_PREFIX: &str =
    "https://github.com/Muhammad-Waleed381/OpenDictate/releases/download/";
static INSTALLING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    /// True when this copy can download and install the update itself.
    pub can_install: bool,
    /// Size of the installer that would be downloaded, in bytes.
    pub download_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    name: String,
    #[serde(default)]
    size: u64,
    browser_download_url: String,
    /// `sha256:<hex>`, published by GitHub for every uploaded asset.
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// How this copy of the app was installed, and so how it can be replaced.
/// Each platform only ever builds some of these.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Method {
    /// Windows NSIS setup (the `-setup.exe`).
    WindowsSetup,
    /// A `.app` bundle on macOS, at the given path.
    MacBundle(PathBuf),
    /// A Linux AppImage file at the given path.
    AppImage(PathBuf),
    /// Installed by dpkg.
    Deb,
    /// Installed by rpm.
    Rpm,
}

/// Picks the release asset that replaces an install made with `method`.
///
/// `cuda` is true in the CUDA build. It only ships as the Linux `.deb` and
/// the Windows setup, named with a `-cuda` suffix, and each build only ever
/// updates to its own kind so a GPU install never drops to the CPU build.
fn pick_asset<'a>(
    method: &Method,
    arch: &str,
    cuda: bool,
    assets: &'a [Asset],
) -> Option<&'a Asset> {
    let find = |matches: &dyn Fn(&str) -> bool| assets.iter().find(|a| matches(&a.name));
    match (method, arch, cuda) {
        (Method::WindowsSetup, "x86_64", false) => find(&|n| n.ends_with("_x64-setup.exe")),
        (Method::WindowsSetup, "x86_64", true) => find(&|n| n.ends_with("_x64-cuda-setup.exe")),
        // The macOS archive is universal: one build for Intel and Apple silicon.
        (Method::MacBundle(_), _, false) => find(&|n| n.ends_with(".app.tar.gz")),
        (Method::AppImage(_), "x86_64", false) => {
            find(&|n| n.ends_with(".AppImage") && n.contains("amd64"))
        }
        (Method::Deb, "x86_64", false) => find(&|n| n.ends_with("_amd64.deb")),
        (Method::Deb, "x86_64", true) => find(&|n| n.ends_with("_amd64-cuda.deb")),
        (Method::Rpm, "x86_64", false) => find(&|n| n.ends_with(".x86_64.rpm")),
        _ => None,
    }
}

/// The hex SHA-256 from a GitHub `digest` field, or `None` for anything else.
fn parse_digest(digest: Option<&str>) -> Option<String> {
    let hex = digest?.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

#[cfg(target_os = "windows")]
fn detect_method() -> Option<Method> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    // The NSIS setup registers an uninstall entry named after the product.
    // An MSI install registers a product code instead, and must not be
    // replaced by the NSIS setup.
    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenDictate";
    let installed = RegKey::predef(HKEY_CURRENT_USER).open_subkey(KEY).is_ok()
        || RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(KEY).is_ok();
    installed.then_some(Method::WindowsSetup)
}

#[cfg(target_os = "macos")]
fn detect_method() -> Option<Method> {
    let exe = std::env::current_exe().ok()?;
    // .../OpenDictate.app/Contents/MacOS/opendictate
    let bundle = exe.ancestors().nth(3)?;
    (bundle.extension()? == "app").then(|| Method::MacBundle(bundle.to_path_buf()))
}

#[cfg(target_os = "linux")]
fn detect_method() -> Option<Method> {
    if let Some(path) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
        return path.is_file().then_some(Method::AppImage(path));
    }
    let exe = std::env::current_exe().ok()?;
    let owned_by = |tool: &str, flag: &str| {
        Command::new(tool)
            .arg(flag)
            .arg(&exe)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    if owned_by("dpkg", "-S") {
        Some(Method::Deb)
    } else if owned_by("rpm", "-qf") {
        Some(Method::Rpm)
    } else {
        None
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn detect_method() -> Option<Method> {
    None
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
        can_install: false,
        download_size: None,
    })
}

fn fetch_latest_release() -> Result<LatestRelease, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .new_agent();
    agent
        .get(LATEST_RELEASE_API)
        // GitHub rejects requests without a User-Agent.
        .header("User-Agent", concat!("OpenDictate/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("could not reach GitHub: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("unexpected reply from GitHub: {e}"))
}

/// The asset this install would use, only when its download can be checked.
fn installable(release: &LatestRelease) -> Option<(Method, &Asset, String)> {
    let method = detect_method()?;
    let asset = pick_asset(
        &method,
        std::env::consts::ARCH,
        crate::state::GPU_BUILD,
        &release.assets,
    )?;
    if !asset.browser_download_url.starts_with(DOWNLOAD_PREFIX) {
        return None;
    }
    let digest = parse_digest(asset.digest.as_deref())?;
    Some((method, asset, digest))
}

#[tauri::command]
pub async fn check_for_update() -> Result<UpdateInfo, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let release = fetch_latest_release()?;
        let mut info = compare(CURRENT_VERSION, &release.tag_name)?;
        if info.update_available {
            if let Some((_, asset, _)) = installable(&release) {
                info.can_install = true;
                info.download_size = Some(asset.size);
            }
        }
        Ok(info)
    })
    .await
    .map_err(|e| format!("update check worker failed: {e}"))?
}

#[derive(Clone, Serialize)]
struct Progress<'a> {
    /// `downloading`, `verifying` or `installing`.
    stage: &'a str,
    downloaded: u64,
    total: u64,
}

fn report(app: &AppHandle, stage: &str, downloaded: u64, total: u64) {
    let _ = app.emit(
        "update-progress",
        Progress {
            stage,
            downloaded,
            total,
        },
    );
}

/// Streams `url` to `dest`, hashing as it goes, and deletes the file unless
/// its SHA-256 matches `expected`.
fn download_verified(
    url: &str,
    expected: &str,
    dest: &Path,
    total: u64,
    mut on_progress: impl FnMut(u64),
) -> Result<(), String> {
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .timeout_global(Some(std::time::Duration::from_secs(30 * 60)))
        .build()
        .new_agent();
    let mut response = agent
        .get(url)
        .header("User-Agent", concat!("OpenDictate/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let mut reader = response.body_mut().as_reader();
    let mut file = std::fs::File::create(dest).map_err(|e| format!("could not save the update: {e}"))?;
    let result = (|| {
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut downloaded = 0u64;
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|e| format!("download interrupted: {e}"))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            file.write_all(&buffer[..read])
                .map_err(|e| format!("could not save the update: {e}"))?;
            downloaded += read as u64;
            on_progress(downloaded);
        }
        file.flush().map_err(|e| format!("could not save the update: {e}"))?;
        if total > 0 && downloaded != total {
            return Err("the download was incomplete".to_string());
        }
        let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if actual != expected {
            return Err("the download did not match its checksum, so it was discarded".to_string());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    result
}

/// Runs `sh -c script target` detached, after this process has had a moment
/// to exit so the single-instance guard does not hand the new start to the
/// closing app.
#[cfg(unix)]
fn restart_with(script: &str, target: &Path) -> Result<(), String> {
    Command::new("sh")
        .arg("-c")
        .arg(script)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not restart the app: {e}"))
}

fn run(command: &mut Command, what: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|e| format!("{what} could not start: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = detail.trim();
    Err(if detail.is_empty() {
        format!("{what} failed ({})", output.status)
    } else {
        format!("{what} failed: {detail}")
    })
}

fn install(method: &Method, package: &Path) -> Result<(), String> {
    match method {
        Method::WindowsSetup => {
            // Passive: a small progress window, no questions. The setup
            // closes the running app and starts the new version itself.
            Command::new(package)
                .args(["/P", "/R"])
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("could not start the installer: {e}"))
        }
        Method::Deb | Method::Rpm => {
            let mut command = Command::new("pkexec");
            if *method == Method::Deb {
                command.args(["dpkg", "-i"]);
            } else {
                command.args(["rpm", "-U"]);
            }
            run(command.arg(package), "installing the package")?;
            #[cfg(unix)]
            {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                return restart_with("sleep 1; exec \"$0\"", &exe);
            }
            #[allow(unreachable_code)]
            Ok(())
        }
        Method::AppImage(target) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(package, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| format!("could not prepare the update: {e}"))?;
                // Same directory, so the rename is atomic and the running
                // file stays usable until this process exits.
                std::fs::rename(package, target)
                    .map_err(|e| format!("could not replace {}: {e}", target.display()))?;
                return restart_with("sleep 1; exec \"$0\"", target);
            }
            #[allow(unreachable_code)]
            Ok(())
        }
        Method::MacBundle(bundle) => {
            let work = package.parent().ok_or("no update folder")?;
            let unpacked = work.join("unpacked");
            let _ = std::fs::remove_dir_all(&unpacked);
            std::fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
            run(
                Command::new("tar").arg("-xzf").arg(package).arg("-C").arg(&unpacked),
                "unpacking the update",
            )?;
            let fresh = std::fs::read_dir(&unpacked)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|x| x == "app"))
                .ok_or("the update did not contain an app")?;
            let parent = bundle.parent().ok_or("no install folder")?;
            let staged = parent.join(".OpenDictate-update.app");
            let old = parent.join(".OpenDictate-old.app");
            let _ = std::fs::remove_dir_all(&staged);
            let _ = std::fs::remove_dir_all(&old);
            // ditto keeps the bundle's permissions and links intact.
            run(
                Command::new("ditto").arg(&fresh).arg(&staged),
                "copying the update into place",
            )
            .map_err(|e| format!("{e} (is {} writable?)", parent.display()))?;
            std::fs::rename(bundle, &old)
                .map_err(|e| format!("could not replace the app: {e}"))?;
            if let Err(e) = std::fs::rename(&staged, bundle) {
                let _ = std::fs::rename(&old, bundle);
                return Err(format!("could not replace the app: {e}"));
            }
            let _ = std::fs::remove_dir_all(&old);
            #[cfg(unix)]
            return restart_with("sleep 1; exec open \"$0\"", bundle);
            #[allow(unreachable_code)]
            Ok(())
        }
    }
}

struct InstallGuard;

impl InstallGuard {
    fn acquire() -> Result<Self, String> {
        INSTALLING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| InstallGuard)
            .map_err(|_| "An update is already being installed.".to_string())
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::SeqCst);
    }
}

fn run_install(app: &AppHandle) -> Result<(), String> {
    let _guard = InstallGuard::acquire()?;
    // The release is looked up again here rather than trusting the page, so
    // only an asset of the newest release can ever be installed.
    let release = fetch_latest_release()?;
    let info = compare(CURRENT_VERSION, &release.tag_name)?;
    if !info.update_available {
        return Err("You already have the latest version.".to_string());
    }
    let (method, asset, digest) =
        installable(&release).ok_or("This copy of OpenDictate cannot update itself.")?;

    let work = match &method {
        // Next to the AppImage, so replacing it is a rename on one volume.
        Method::AppImage(path) => path.parent().map(Path::to_path_buf),
        _ => app.path().app_cache_dir().ok().map(|d| d.join("update")),
    }
    .ok_or("no folder for the update")?;
    std::fs::create_dir_all(&work).map_err(|e| format!("could not prepare the update: {e}"))?;
    // The name comes from GitHub; keep only its last path component.
    let file_name = Path::new(&asset.name)
        .file_name()
        .ok_or("unexpected installer name")?;
    let package = work.join(file_name);
    let _ = std::fs::remove_file(&package);

    let total = asset.size;
    let mut last_report = std::time::Instant::now();
    report(app, "downloading", 0, total);
    download_verified(&asset.browser_download_url, &digest, &package, total, |done| {
        if last_report.elapsed() >= std::time::Duration::from_millis(100) {
            last_report = std::time::Instant::now();
            report(app, "downloading", done, total);
        }
    })?;
    report(app, "installing", total, total);
    if let Err(e) = install(&method, &package) {
        let _ = std::fs::remove_file(&package);
        return Err(e);
    }
    // The installer owns the file now on Windows, and the other platforms
    // are done with it; either way the app is about to exit.
    Ok(())
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<(), String> {
    if state.user_dictation_active.load(Ordering::SeqCst) {
        return Err("Finish your dictation before updating.".to_string());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || run_install(&handle))
        .await
        .map_err(|e| format!("update worker failed: {e}"))??;
    // Windows installers cannot replace a running app, and the others
    // restart it, so the app always closes here.
    app.exit(0);
    Ok(())
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
        compare(CURRENT_VERSION, &fetch_latest_release().unwrap().tag_name).unwrap();
    }

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.into(),
            size: 1,
            browser_download_url: format!("{DOWNLOAD_PREFIX}v0.4.0/{name}"),
            digest: None,
        }
    }

    fn release_assets() -> Vec<Asset> {
        [
            "OpenDictate-0.4.0-1.x86_64.rpm",
            "OpenDictate_0.4.0_amd64.AppImage",
            "OpenDictate_0.4.0_amd64.deb",
            "OpenDictate_0.4.0_universal.dmg",
            "OpenDictate_0.4.0_x64-setup.exe",
            "OpenDictate_0.4.0_x64_en-US.msi",
            "OpenDictate_universal.app.tar.gz",
            "OpenDictate_0.4.0_amd64-cuda.deb",
            "OpenDictate_0.4.0_x64-cuda-setup.exe",
        ]
        .map(asset)
        .to_vec()
    }

    #[test]
    fn each_install_method_gets_its_own_package() {
        let assets = release_assets();
        let name = |method: Method, arch: &str| {
            pick_asset(&method, arch, false, &assets).map(|a| a.name.clone())
        };
        assert_eq!(
            name(Method::WindowsSetup, "x86_64").as_deref(),
            Some("OpenDictate_0.4.0_x64-setup.exe")
        );
        assert_eq!(
            name(Method::Deb, "x86_64").as_deref(),
            Some("OpenDictate_0.4.0_amd64.deb")
        );
        assert_eq!(
            name(Method::Rpm, "x86_64").as_deref(),
            Some("OpenDictate-0.4.0-1.x86_64.rpm")
        );
        assert_eq!(
            name(Method::AppImage("/a.AppImage".into()), "x86_64").as_deref(),
            Some("OpenDictate_0.4.0_amd64.AppImage")
        );
        for arch in ["x86_64", "aarch64"] {
            assert_eq!(
                name(Method::MacBundle("/Applications/OpenDictate.app".into()), arch)
                    .as_deref(),
                Some("OpenDictate_universal.app.tar.gz")
            );
        }
    }

    #[test]
    fn the_cuda_build_only_updates_to_cuda_packages() {
        let assets = release_assets();
        let name = |method: Method| {
            pick_asset(&method, "x86_64", true, &assets).map(|a| a.name.clone())
        };
        assert_eq!(
            name(Method::Deb).as_deref(),
            Some("OpenDictate_0.4.0_amd64-cuda.deb")
        );
        assert_eq!(
            name(Method::WindowsSetup).as_deref(),
            Some("OpenDictate_0.4.0_x64-cuda-setup.exe")
        );
        // No CUDA build exists for these, so the app falls back to the page.
        assert!(name(Method::Rpm).is_none());
        assert!(name(Method::AppImage("/a.AppImage".into())).is_none());
        assert!(name(Method::MacBundle("/Applications/OpenDictate.app".into())).is_none());
    }

    #[test]
    fn architectures_without_a_build_get_no_package() {
        let assets = release_assets();
        for method in [Method::WindowsSetup, Method::Deb, Method::Rpm] {
            assert!(pick_asset(&method, "aarch64", false, &assets).is_none());
        }
        assert!(pick_asset(&Method::Deb, "x86_64", false, &[]).is_none());
    }

    #[test]
    fn only_a_well_formed_sha256_digest_is_accepted() {
        let hex = "a".repeat(64);
        assert_eq!(parse_digest(Some(&format!("sha256:{hex}"))), Some(hex.clone()));
        assert_eq!(
            parse_digest(Some(&format!("sha256:{}", hex.to_uppercase()))),
            Some(hex)
        );
        for bad in [None, Some(""), Some("sha1:abc"), Some("sha256:abc"), Some("sha256:zz")] {
            assert_eq!(parse_digest(bad), None);
        }
    }

    /// Serves `body` to every request on a local port and returns its URL.
    fn serve(body: Vec<u8>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/installer", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut request = [0u8; 2048];
                let _ = stream.read(&mut request);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        url
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn a_matching_download_is_kept_and_anything_else_is_deleted() {
        let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let url = serve(body.clone());
        let dir = std::env::temp_dir().join(format!("od-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("package");
        let size = body.len() as u64;

        let mut seen = 0;
        download_verified(&url, &sha256_hex(&body), &dest, size, |n| seen = n).unwrap();
        assert_eq!(seen, size);
        assert_eq!(std::fs::read(&dest).unwrap(), body);

        // A wrong checksum must not leave the file behind.
        let wrong = "0".repeat(64);
        let error = download_verified(&url, &wrong, &dest, size, |_| {}).unwrap_err();
        assert!(error.contains("checksum"), "{error}");
        assert!(!dest.exists());

        // Neither may a download that is shorter than the release says.
        let error = download_verified(&url, &sha256_hex(&body), &dest, size + 1, |_| {})
            .unwrap_err();
        assert!(error.contains("incomplete"), "{error}");
        assert!(!dest.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn an_appimage_update_replaces_the_file_in_place_and_stays_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("od-appimage-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (target, package) = (dir.join("OpenDictate.AppImage"), dir.join("new.AppImage"));
        std::fs::write(&target, "#!/bin/sh\nexit 0\n").unwrap();
        // Downloads are written without the executable bit.
        std::fs::write(&package, "#!/bin/sh\n# new version\nexit 0\n").unwrap();

        install(&Method::AppImage(target.clone()), &package).unwrap();

        assert!(std::fs::read_to_string(&target).unwrap().contains("new version"));
        assert!(!package.exists());
        assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o111, 0o111);
        std::thread::sleep(std::time::Duration::from_millis(1300));
        let _ = std::fs::remove_dir_all(dir);
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
