//! Profile Management
//!
//! Stores and loads per-game profiles that control audio routing, device DPI,
//! RGB colors, and other settings. Profiles are JSON files in the config dir.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A complete device/audio profile for a specific game or use case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    /// Executable names that trigger this profile (e.g., ["cs2", "csgo_linux64"])
    #[serde(default)]
    pub executables: Vec<String>,

    #[serde(default)]
    pub audio: AudioProfile,
    #[serde(default)]
    pub mouse: MouseProfile,
    #[serde(default)]
    pub rgb: RGBProfile,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioProfile {
    /// Per-channel volumes (0–150)
    #[serde(default)]
    pub volumes: HashMap<String, u32>,
    /// Per-channel EQ presets
    #[serde(default)]
    pub eq_presets: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MouseProfile {
    pub dpi: Option<u32>,
    pub polling_rate: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RGBProfile {
    /// Color in #RRGGBB format
    pub color: Option<String>,
    /// Mode name (e.g., "static", "breathing", "wave")
    pub mode: Option<String>,
}

/// Manages loading, saving, and matching profiles.
pub struct ProfileManager {
    profiles_dir: PathBuf,
    profiles: Vec<Profile>,
    /// Map of executable name → profile index for fast lookup
    exe_map: HashMap<String, usize>,
}

impl ProfileManager {
    /// Load all profiles from the profiles directory.
    pub fn new(profiles_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(profiles_dir)?;

        let mut profiles = Vec::new();
        let mut exe_map = HashMap::new();

        if profiles_dir.exists() {
            for entry in std::fs::read_dir(profiles_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    match std::fs::read_to_string(&path)
                        .and_then(|s| serde_json::from_str::<Profile>(&s).map_err(Into::into))
                    {
                        Ok(profile) => {
                            let idx = profiles.len();
                            for exe in &profile.executables {
                                exe_map.insert(exe.clone(), idx);
                            }
                            tracing::debug!("Loaded profile: {} ({:?})", profile.name, profile.executables);
                            profiles.push(profile);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load profile {}: {e}", path.display());
                        }
                    }
                }
            }
        }

        tracing::info!("Loaded {} profiles from {}", profiles.len(), profiles_dir.display());

        Ok(Self {
            profiles_dir: profiles_dir.to_owned(),
            profiles,
            exe_map,
        })
    }

    /// Find a profile matching a given executable name.
    pub fn match_executable(&self, exe_name: &str) -> Option<&Profile> {
        // Direct match
        if let Some(&idx) = self.exe_map.get(exe_name) {
            return Some(&self.profiles[idx]);
        }
        // Substring match (e.g., "cs2" matches "cs2.exe" or "/path/to/cs2")
        let exe_lower = exe_name.to_lowercase();
        for (key, &idx) in &self.exe_map {
            if exe_lower.contains(&key.to_lowercase()) {
                return Some(&self.profiles[idx]);
            }
        }
        None
    }

    /// Save a new or updated profile.
    pub fn save_profile(&mut self, profile: Profile) -> Result<()> {
        let filename = profile.name.to_lowercase().replace(' ', "_") + ".json";
        let path = self.profiles_dir.join(&filename);
        let json = serde_json::to_string_pretty(&profile)?;
        std::fs::write(&path, json)?;

        // Update in-memory index
        let idx = self.profiles.len();
        for exe in &profile.executables {
            self.exe_map.insert(exe.clone(), idx);
        }
        self.profiles.push(profile);

        tracing::info!("Saved profile to {}", path.display());
        Ok(())
    }

    /// Get all profiles.
    pub fn list(&self) -> &[Profile] {
        &self.profiles
    }

    /// Create a default example profile.
    pub fn create_example_profile(profiles_dir: &Path) -> Result<()> {
        let example = Profile {
            name: "CS2 Competitive".into(),
            executables: vec!["cs2".into(), "csgo_linux64".into()],
            audio: AudioProfile {
                volumes: [
                    ("Game".into(), 100),
                    ("Chat".into(), 80),
                    ("Media".into(), 30),
                ]
                .into(),
                eq_presets: [("Game".into(), "FPS Footsteps".into())].into(),
            },
            mouse: MouseProfile {
                dpi: Some(800),
                polling_rate: Some(1000),
            },
            rgb: RGBProfile {
                color: Some("#FF6600".into()),
                mode: Some("static".into()),
            },
        };

        let path = profiles_dir.join("cs2_competitive.json");
        if !path.exists() {
            std::fs::create_dir_all(profiles_dir)?;
            std::fs::write(&path, serde_json::to_string_pretty(&example)?)?;
            tracing::info!("Created example profile at {}", path.display());
        }
        Ok(())
    }
}
