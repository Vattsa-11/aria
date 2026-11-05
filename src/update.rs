use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const REPO: &str = "Vattsa-11/aria";
const LATEST: &str = "https://api.github.com/repos/Vattsa-11/aria/releases/latest";
const TRUSTED_DOWNLOADS: &str = "https://github.com/Vattsa-11/aria/releases/download/";

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sum_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub page: String,
    pub asset: Option<Asset>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading(Release),
    Ready(PathBuf),
    Failed(String),
}

pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let mut next = || -> Option<u64> {
        let part = parts.next()?;
        part.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
    };
    Some((next()?, next()?, next()?))
}

pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    }
}

pub fn pick_asset(assets: &[Asset], os: &str, arch: &str, has_dpkg: bool) -> Option<Asset> {
    let aliases: &[&str] = match arch {
        "x86_64" => &["x86_64", "amd64"],
        "aarch64" => &["aarch64", "arm64"],
        other => &[other],
    };
    let find = |suffix: &str| {
        assets
            .iter()
            .find(|asset| asset.name.contains(os) && aliases.iter().any(|alias| asset.name.contains(alias)) && asset.name.ends_with(suffix))
            .cloned()
    };
    match os {
        "windows" => find(".msi").or_else(|| find(".exe")),
        "linux" if has_dpkg => find(".deb").or_else(|| find(".tar.gz")),
        _ => find(".tar.gz"),
    }
}

fn platform() -> (&'static str, &'static str) {
    (std::env::consts::OS, std::env::consts::ARCH)
}

pub fn release_from_json(text: &str) -> Result<Release, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let version = value["tag_name"].as_str().ok_or("no tag_name in the release")?.trim_start_matches('v').to_string();
    let listed: Vec<(String, String, u64)> = value["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let url = item["browser_download_url"].as_str()?.to_string();
                    url.starts_with(TRUSTED_DOWNLOADS).then(|| (item["name"].as_str().unwrap_or_default().to_string(), url, item["size"].as_u64().unwrap_or(0)))
                })
                .collect()
        })
        .unwrap_or_default();
    let assets: Vec<Asset> = listed
        .iter()
        .filter(|(name, _, _)| !name.ends_with(".sha256") && name != "SHA256SUMS.txt")
        .map(|(name, url, size)| Asset {
            name: name.clone(),
            url: url.clone(),
            size: *size,
            sum_url: listed.iter().find(|(other, _, _)| *other == format!("{name}.sha256")).map(|(_, sum, _)| sum.clone()),
        })
        .collect();
    let (os, arch) = platform();
    let has_dpkg = which_dpkg();
    Ok(Release {
        page: value["html_url"].as_str().unwrap_or("https://github.com/Vattsa-11/aria/releases/latest").to_string(),
        asset: pick_asset(&assets, os, arch, has_dpkg),
        version,
    })
}

fn which_dpkg() -> bool {
    cfg!(target_os = "linux") && crate::proc::command("dpkg").arg("--version").output().is_ok()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn check() -> Result<Release, String> {
    let mut response = crate::http::fetch_agent()
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", concat!("Aria/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|err| format!("Could not reach GitHub: {err}"))?;
    let text = response.body_mut().read_to_string().map_err(|err| err.to_string())?;
    release_from_json(&text)
}

pub fn dir() -> PathBuf {
    crate::settings::data_dir().join("updates")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn download(asset: &Asset) -> Result<PathBuf, String> {
    if !asset.url.starts_with(TRUSTED_DOWNLOADS) {
        return Err(format!("The download is not hosted by {REPO}"));
    }
    let folder = dir();
    std::fs::create_dir_all(&folder).map_err(|err| err.to_string())?;
    let target = folder.join(&asset.name);
    let part = folder.join(format!("{}.part", asset.name));
    let mut response = crate::http::stream_agent()
        .get(&asset.url)
        .header("User-Agent", concat!("Aria/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|err| format!("Download failed: {err}"))?;
    let expected = match &asset.sum_url {
        Some(url) => fetch_digest(url)?,
        None => return Err("This release has no published checksum for the download, so Aria will not install it".to_string()),
    };
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    let mut file = std::fs::File::create(&part).map_err(|err| err.to_string())?;
    let mut reader = response.body_mut().as_reader();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let read = reader.read(&mut buffer).map_err(|err| format!("Download failed: {err}"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|err| err.to_string())?;
        sha2::Digest::update(&mut hasher, &buffer[..read]);
        written += read as u64;
    }
    drop(file);
    if asset.size > 0 && written != asset.size {
        let _ = std::fs::remove_file(&part);
        return Err(format!("Download was cut short ({written} of {} bytes)", asset.size));
    }
    let actual = hex(&sha2::Digest::finalize(hasher));
    if !actual.eq_ignore_ascii_case(&expected) {
        let _ = std::fs::remove_file(&part);
        return Err("The download does not match its published checksum, so it was discarded".to_string());
    }
    std::fs::rename(&part, &target).map_err(|err| err.to_string())?;
    Ok(target)
}

pub fn digest_from(text: &str) -> Option<String> {
    let token = text.split_whitespace().next()?;
    (token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit())).then(|| token.to_ascii_lowercase())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_digest(url: &str) -> Result<String, String> {
    if !url.starts_with(TRUSTED_DOWNLOADS) {
        return Err(format!("The checksum is not hosted by {REPO}"));
    }
    let mut response = crate::http::fetch_agent()
        .get(url)
        .header("User-Agent", concat!("Aria/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|err| format!("Could not fetch the checksum: {err}"))?;
    let text = response.body_mut().read_to_string().map_err(|err| err.to_string())?;
    digest_from(&text).ok_or_else(|| "The published checksum is not valid".to_string())
}

pub fn launch(path: &Path) -> Result<bool, String> {
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    let spawned = if cfg!(target_os = "windows") && name.ends_with(".msi") {
        crate::proc::command("msiexec").arg("/i").arg(path).spawn().map(|_| true)
    } else if cfg!(target_os = "windows") {
        crate::proc::command(path).spawn().map(|_| true)
    } else if name.ends_with(".tar.gz") {
        let folder = path.parent().unwrap_or(path);
        open_with_system(folder).map(|_| false)
    } else {
        open_with_system(path).map(|_| false)
    };
    spawned.map_err(|err| format!("Could not start the installer: {err}"))
}

fn open_with_system(path: &Path) -> std::io::Result<()> {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    crate::proc::command(opener).arg(path).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset { name: name.to_string(), url: format!("https://x/{name}"), size: 10, sum_url: None }
    }

    #[test]
    fn versions_compare_numerically_not_alphabetically() {
        assert!(is_newer("1.0.1", "v1.0.2"));
        assert!(is_newer("1.0.9", "1.0.10"));
        assert!(is_newer("1.9.0", "v2.0.0"));
        assert!(!is_newer("1.0.2", "1.0.2"));
        assert!(!is_newer("1.0.2", "1.0.1"));
        assert!(!is_newer("1.0.2", "preview"));
    }

    #[test]
    fn each_platform_gets_its_own_installer() {
        let all = [
            asset("aria-windows-x86_64.exe"),
            asset("aria-windows-x86_64.msi"),
            asset("aria-windows-x86_64.zip"),
            asset("aria-linux-amd64.deb"),
            asset("aria-linux-x86_64.tar.gz"),
            asset("aria-macos-aarch64.tar.gz"),
            asset("SHA256SUMS.txt"),
        ];
        assert_eq!(pick_asset(&all, "windows", "x86_64", false).unwrap().name, "aria-windows-x86_64.msi");
        assert_eq!(pick_asset(&all, "linux", "x86_64", true).unwrap().name, "aria-linux-amd64.deb");
        assert_eq!(pick_asset(&all, "linux", "x86_64", false).unwrap().name, "aria-linux-x86_64.tar.gz");
        assert_eq!(pick_asset(&all, "macos", "aarch64", false).unwrap().name, "aria-macos-aarch64.tar.gz");
        assert!(pick_asset(&all, "macos", "x86_64", false).is_none());
    }

    #[test]
    fn parses_a_github_release_payload() {
        let base = "https://github.com/Vattsa-11/aria/releases/download/v1.2.3";
        let name = format!("aria-{}-{}.tar.gz", platform().0, platform().1);
        let json = format!(
            r#"{{"tag_name":"v1.2.3","html_url":"https://github.com/Vattsa-11/aria/releases/tag/v1.2.3","assets":[{{"name":"{name}","browser_download_url":"{base}/{name}","size":5}},{{"name":"{name}.sha256","browser_download_url":"{base}/{name}.sha256","size":90}},{{"name":"evil.msi","browser_download_url":"https://evil.example/evil.msi","size":5}}]}}"#
        );
        let release = release_from_json(&json).unwrap();
        assert_eq!(release.version, "1.2.3");
        assert!(release_from_json("{}").is_err());
        if let Some(asset) = release.asset {
            assert_eq!(asset.sum_url.as_deref(), Some(format!("{base}/{name}.sha256").as_str()));
            assert!(asset.url.starts_with(TRUSTED_DOWNLOADS));
        }
    }

    #[test]
    fn assets_from_other_hosts_are_ignored() {
        let json = r#"{"tag_name":"v9.9.9","assets":[{"name":"aria-windows-x86_64.msi","browser_download_url":"https://evil.example/aria-windows-x86_64.msi","size":5}]}"#;
        assert!(release_from_json(json).unwrap().asset.is_none());
    }

    #[test]
    fn checksums_must_be_64_hex_digits() {
        let good = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  aria.msi\n";
        assert_eq!(digest_from(good).as_deref(), Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"));
        assert!(digest_from("zzzz  aria.msi").is_none());
        assert!(digest_from("").is_none());
        assert_eq!(hex(&[0, 15, 255]), "000fff");
    }
}
