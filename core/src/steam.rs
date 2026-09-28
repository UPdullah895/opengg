//! Steam game detection and metadata management.
//!
//! Moved verbatim from `frontend/src-tauri/src/commands.rs` (plan §2.1).
//! This module detects installed Steam games, resolves their artwork, and caches
//! metadata from the Steam Store API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Public Steam game entry returned to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamGameEntry {
    pub appid: String,
    pub name: String,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SteamAppDetailsBasic {
    name: String,
    header_image: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SteamAppDetailsEnvelope {
    success: bool,
    data: Option<SteamAppDetailsBasic>,
}

pub fn parse_acf_field(content: &str, field: &str) -> Option<String> {
    for line in content.lines() {
        let t = line.trim();
        if !t.starts_with(&format!("\"{field}\"")) {
            continue;
        }
        let parts: Vec<&str> = t.split('"').collect();
        if parts.len() >= 4 {
            return Some(parts[3].to_string());
        }
    }
    None
}

pub fn steam_librarycache_icon(steam_root: &Path, appid: &str) -> Option<String> {
    let app_dir = steam_root.join("appcache").join("librarycache").join(appid);
    if !app_dir.is_dir() {
        return None;
    }

    for candidate in ["header.jpg", "library_600x900.jpg", "logo.png"] {
        let path = app_dir.join(candidate);
        if path.is_file() {
            return Some(path.to_string_lossy().to_string());
        }
    }

    if let Ok(entries) = std::fs::read_dir(&app_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if ext == "jpg" || ext == "png" || ext == "jpeg" || ext == "webp" {
                    return Some(path.to_string_lossy().to_string());
                }
            }
        }
    }

    None
}

pub fn steam_root_candidates(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".steam/root"),
        home.join(".steam/steam"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ]
}

/// Public function to return all Steam artwork root directories that exist on this machine.
/// Returns directories that the media server should be allowed to serve from:
/// - ~/.local/share/Steam/appcache/librarycache (native Steam)
/// - ~/.local/share/Steam/userdata/*/config/librarycache (user-specific cache)
/// - Equivalent paths for alternate Steam root locations (symlinks, flatpak, etc.)
///
/// Only includes directories that actually exist.
pub fn steam_artwork_roots() -> Vec<PathBuf> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let mut roots = Vec::new();

    // Add appcache/librarycache for each root candidate that exists
    for steam_root in steam_root_candidates(&home) {
        let appcache_dir = steam_root.join("appcache").join("librarycache");
        if appcache_dir.exists() && !roots.contains(&appcache_dir) {
            roots.push(appcache_dir);
        }
    }

    // Add userdata config/librarycache directories
    for user_cache_dir in steam_user_librarycache_dirs(&home) {
        if !roots.contains(&user_cache_dir) {
            roots.push(user_cache_dir);
        }
    }

    roots
}

pub fn parse_loginusers_most_recent_steamid(loginusers_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(loginusers_path).ok()?;
    let mut current_user: Option<String> = None;
    let mut current_is_most_recent = false;
    let mut in_user_block = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.starts_with('"') {
            let parts: Vec<&str> = line.split('"').collect();
            if parts.len() >= 4 {
                let key = parts[1];
                let value = parts[3];
                if key.chars().all(|c| c.is_ascii_digit()) && key.len() >= 10 {
                    current_user = Some(key.to_string());
                    current_is_most_recent = false;
                    in_user_block = true;
                    continue;
                }
                if in_user_block && key == "MostRecent" && value == "1" {
                    current_is_most_recent = true;
                }
            }
        } else if line == "}" && in_user_block {
            if current_is_most_recent {
                return current_user.clone();
            }
            current_user = None;
            current_is_most_recent = false;
            in_user_block = false;
        }
    }

    None
}

pub fn steam64_to_account_id(steamid64: &str) -> Option<String> {
    let id64 = steamid64.parse::<u64>().ok()?;
    let base = 76_561_197_960_265_728_u64;
    id64.checked_sub(base).map(|id| id.to_string())
}

pub fn steam_user_librarycache_dirs(home: &Path) -> Vec<PathBuf> {
    let roots = steam_root_candidates(home);
    let mut dirs = Vec::new();

    for steam_root in &roots {
        let userdata_dir = steam_root.join("userdata");
        if !userdata_dir.is_dir() {
            continue;
        }

        let loginusers = steam_root.join("config/loginusers.vdf");
        if let Some(steamid64) = parse_loginusers_most_recent_steamid(&loginusers) {
            if let Some(account_id) = steam64_to_account_id(&steamid64) {
                let preferred = userdata_dir.join(account_id).join("config/librarycache");
                if preferred.is_dir() && !dirs.contains(&preferred) {
                    dirs.push(preferred);
                    continue;
                }
            }
        }

        if let Ok(entries) = std::fs::read_dir(&userdata_dir) {
            for entry in entries.flatten() {
                let dir = entry.path().join("config/librarycache");
                if dir.is_dir() && !dirs.contains(&dir) {
                    dirs.push(dir);
                }
            }
        }
    }

    dirs
}

pub fn steam_owned_appids(home: &Path) -> Vec<String> {
    let mut owned = std::collections::HashSet::new();

    for cache_dir in steam_user_librarycache_dirs(home) {
        if let Ok(entries) = std::fs::read_dir(cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                if stem.chars().all(|c| c.is_ascii_digit()) {
                    owned.insert(stem.to_string());
                }
            }
        }
    }

    let mut appids: Vec<String> = owned.into_iter().collect();
    appids.sort();
    appids
}

fn steam_metadata_cache_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("opengg/steam-metadata-cache.json")
}

fn load_steam_metadata_cache() -> HashMap<String, SteamAppDetailsBasic> {
    let path = steam_metadata_cache_path();
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<HashMap<String, SteamAppDetailsBasic>>(&text).ok())
        .unwrap_or_default()
}

fn save_steam_metadata_cache(cache: &HashMap<String, SteamAppDetailsBasic>) {
    let path = steam_metadata_cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(cache) {
        let _ = std::fs::write(path, text);
    }
}

async fn fetch_steam_store_details(
    appid: &str,
    client: &reqwest::Client,
) -> Option<SteamAppDetailsBasic> {
    let url = format!("https://store.steampowered.com/api/appdetails?appids={appid}&filters=basic");
    for attempt in 0..3 {
        let fetched = async {
            let res = client
                .get(&url)
                .header(reqwest::header::USER_AGENT, "OpenGG/1.0")
                .send()
                .await
                .ok()?;
            let payload = res
                .json::<HashMap<String, SteamAppDetailsEnvelope>>()
                .await
                .ok()?;
            let envelope = payload.get(appid)?;
            if !envelope.success {
                return None;
            }
            envelope.data.clone()
        }
        .await;
        if fetched.is_some() {
            return fetched;
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(250 * (attempt + 1) as u64)).await;
        }
    }
    None
}

pub async fn get_steam_games() -> Result<Vec<SteamGameEntry>, String> {
    let mut installed_games: HashMap<String, SteamGameEntry> = HashMap::new();
    let mut seen_appids = std::collections::HashSet::new();
    let home = std::env::var("HOME").map_err(|_| "HOME not set")?;
    let home_p = PathBuf::from(&home);

    let mut roots = steam_root_candidates(&home_p);
    // Add possible symlink targets or common custom locations if they exist
    roots.dedup();

    let mut library_folders = Vec::new();

    for steam_root in roots {
        if !steam_root.exists() {
            continue;
        }
        let base_apps = steam_root.join("steamapps");
        if base_apps.exists() && !library_folders.contains(&base_apps) {
            library_folders.push(base_apps);
        }

        // Read libraryfolders.vdf to find additional library paths
        let lf_vdf = steam_root.join("steamapps/libraryfolders.vdf");
        if let Ok(content) = std::fs::read_to_string(lf_vdf) {
            for line in content.lines() {
                let t = line.trim();
                if t.to_lowercase().starts_with("\"path\"") {
                    let parts: Vec<&str> = t.split('\"').collect();
                    if parts.len() >= 4 {
                        let p = PathBuf::from(parts[3]).join("steamapps");
                        if p.exists() && !library_folders.contains(&p) {
                            library_folders.push(p);
                        }
                    }
                }
            }
        }
    }

    // Scan each discovered library for appmanifest_*.acf
    for lib in library_folders {
        if let Ok(entries) = std::fs::read_dir(lib) {
            for entry in entries.flatten() {
                let path = entry.path();
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.starts_with("appmanifest_") && fname.ends_with(".acf") {
                    // Extract appid from filename (appmanifest_<appid>.acf)
                    let appid = fname
                        .strip_prefix("appmanifest_")
                        .and_then(|s| s.strip_suffix(".acf"))
                        .unwrap_or("");

                    if seen_appids.contains(appid) {
                        continue;
                    }
                    seen_appids.insert(appid.to_string());

                    if let Ok(content) = std::fs::read_to_string(path) {
                        let Some(name) = parse_acf_field(&content, "name") else {
                            continue;
                        };
                        let icon_url = steam_librarycache_icon(&home_p.join(".steam/root"), appid)
                            .or_else(|| steam_librarycache_icon(&home_p.join(".steam/steam"), appid))
                            .or_else(|| steam_librarycache_icon(&home_p.join(".local/share/Steam"), appid))
                            .or_else(|| steam_librarycache_icon(&home_p.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"), appid))
                            .or_else(|| {
                                parse_acf_field(&content, "icon")
                                    .filter(|hash| !hash.is_empty())
                                    .map(|hash| format!(
                                        "https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/{}/{}.jpg",
                                        appid,
                                        hash
                                    ))
                            });
                        installed_games.insert(
                            appid.to_string(),
                            SteamGameEntry {
                                appid: appid.to_string(),
                                name,
                                icon_url,
                            },
                        );
                    }
                }
            }
        }
    }

    let mut appids = steam_owned_appids(&home_p);
    for appid in installed_games.keys() {
        if !appids.contains(appid) {
            appids.push(appid.clone());
        }
    }
    appids.sort();
    appids.dedup();

    let mut metadata_cache = load_steam_metadata_cache();
    let cache_snapshot = Arc::new(metadata_cache.clone());
    let client = Arc::new(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .map_err(|e| format!("steam metadata client: {e}"))?,
    );
    let semaphore = Arc::new(tokio::sync::Semaphore::new(4));
    let mut tasks = tokio::task::JoinSet::new();

    for appid in appids {
        let client = Arc::clone(&client);
        let semaphore = Arc::clone(&semaphore);
        let installed = installed_games.get(&appid).cloned();
        let cached = cache_snapshot.get(&appid).cloned();
        let home = home_p.clone();
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok()?;
            let local_icon = steam_librarycache_icon(&home.join(".steam/root"), &appid)
                .or_else(|| steam_librarycache_icon(&home.join(".steam/steam"), &appid))
                .or_else(|| steam_librarycache_icon(&home.join(".local/share/Steam"), &appid))
                .or_else(|| {
                    steam_librarycache_icon(
                        &home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
                        &appid,
                    )
                });
            let store = fetch_steam_store_details(&appid, &client).await;
            let metadata = store.clone().or(cached.clone());

            let name = metadata
                .as_ref()
                .map(|data| data.name.clone())
                .or_else(|| installed.as_ref().map(|game| game.name.clone()))
                .unwrap_or_else(|| format!("Steam App {}", appid));
            let icon_url = local_icon
                .or_else(|| metadata.as_ref().and_then(|data| data.header_image.clone()))
                .or_else(|| installed.as_ref().and_then(|game| game.icon_url.clone()));

            Some((SteamGameEntry { appid, name, icon_url }, store))
        });
    }

    let mut games = Vec::new();
    while let Some(result) = tasks.join_next().await {
        if let Ok(Some((game, store))) = result {
            if let Some(store) = store {
                metadata_cache.insert(game.appid.clone(), store);
            }
            games.push(game);
        }
    }
    save_steam_metadata_cache(&metadata_cache);

    games.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.name.cmp(&b.name))
    });
    Ok(games)
}
