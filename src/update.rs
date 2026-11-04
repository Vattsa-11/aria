use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const LATEST: &str = "https://api.github.com/repos/magi8101/aria/releases/latest";

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
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
    let assets: Vec<Asset> = value["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    Some(Asset {
                        name: item["name"].as_str()?.to_string(),
                        url: item["browser_download_url"].as_str()?.to_string(),
                        size: item["size"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let (os, arch) = platform();
    let has_dpkg = which_dpkg();
    Ok(Release {
        page: value["html_url"].as_str().unwrap_or("https://github.com/magi8101/aria/releases/latest").to_string(),
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
    let folder = dir();
    std::fs::create_dir_all(&folder).map_err(|err| err.to_string())?;
    let target = folder.join(&asset.name);
    let part = folder.join(format!("{}.part", asset.name));
    let mut response = crate::http::stream_agent()
        .get(&asset.url)
        .header("User-Agent", concat!("Aria/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|err| format!("Download failed: {err}"))?;
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
        written += read as u64;
    }
    drop(file);
    if asset.size > 0 && written != asset.size {
        let _ = std::fs::remove_file(&part);
        return Err(format!("Download was cut short ({written} of {} bytes)", asset.size));
    }
    std::fs::rename(&part, &target).map_err(|err| err.to_string())?;
    Ok(target)
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
        Asset { name: name.to_string(), url: format!("https://x/{name}"), size: 10 }
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
        let json = r#"{"tag_name":"v1.2.3","html_url":"https://github.com/magi8101/aria/releases/tag/v1.2.3","assets":[{"name":"a.msi","browser_download_url":"https://x/a.msi","size":5}]}"#;
        let release = release_from_json(json).unwrap();
        assert_eq!(release.version, "1.2.3");
        assert!(release_from_json("{}").is_err());
    }
}
