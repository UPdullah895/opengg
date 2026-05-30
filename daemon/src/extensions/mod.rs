use std::os::unix::fs::PermissionsExt;

/// Scan `~/.local/share/opengg/extensions/` for subdirectories containing a
/// `manifest.json` with a `daemon` field, then supervise each matching
/// executable in its own tokio task with exponential-backoff restart logic.
pub async fn start() {
    let ext_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.local/share"))
        .join("opengg/extensions");

    if !ext_dir.exists() {
        return;
    }

    let Ok(entries) = std::fs::read_dir(&ext_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("manifest.json");
        let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };

        let name = v["name"]
            .as_str()
            .or_else(|| v["id"].as_str())
            .unwrap_or("unknown")
            .to_string();

        // Skip frontend-only extensions (no `daemon` field).
        let Some(daemon_rel) = v["daemon"].as_str() else {
            continue;
        };

        let bin = path.join(daemon_rel);

        match std::fs::metadata(&bin) {
            Err(_) => {
                tracing::warn!("Extension {name}: daemon binary {:?} not found — skipping", bin);
                continue;
            }
            Ok(meta) if meta.permissions().mode() & 0o111 == 0 => {
                tracing::warn!("Extension {name}: {:?} is not executable — skipping", bin);
                continue;
            }
            Ok(_) => {}
        }

        tracing::info!("Extension: loading {name}");
        tokio::spawn(supervise(bin, name));
    }
}

async fn supervise(bin: std::path::PathBuf, name: String) {
    let mut delay_secs: u64 = 1;
    loop {
        tracing::info!("Extension {name}: starting");
        match tokio::process::Command::new(&bin).spawn() {
            Err(e) => tracing::error!("Extension {name}: spawn failed: {e}"),
            Ok(mut child) => match child.wait().await {
                Ok(status) => tracing::warn!(
                    "Extension {name}: exited ({status}), restarting in {delay_secs}s"
                ),
                Err(e) => tracing::error!("Extension {name}: wait error: {e}"),
            },
        }
        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
        delay_secs = (delay_secs * 2).min(60);
    }
}
