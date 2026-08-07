use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

use crate::provider::Provider;

/// API keys for Direct-mode providers. Subscription providers keep their
/// OAuth tokens in claude-code-proxy's own store; we never copy them here.
#[derive(Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    api_keys: BTreeMap<String, String>,
}

pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("AGPX_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("agpx")
}

fn creds_path() -> PathBuf {
    config_dir().join("credentials.json")
}

fn load() -> Result<Store> {
    let path = creds_path();
    if !path.exists() {
        return Ok(Store::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("{} is not valid JSON", path.display()))
}

fn save(store: &Store) -> Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("could not create {}", dir.display()))?;
    let path = creds_path();

    // Create with 0600 up front so the key is never briefly world-readable.
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&path)
        .with_context(|| format!("could not write {}", path.display()))?;
    file.write_all(serde_json::to_string_pretty(store)?.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

/// Env var an existing key may already be sitting in, checked before our store
/// so agpx fits into environments that already export one.
fn env_var(provider: Provider) -> Option<&'static str> {
    match provider {
        Provider::Deepseek => Some("DEEPSEEK_API_KEY"),
        _ => None,
    }
}

pub fn api_key(provider: Provider) -> Result<String> {
    if let Some(var) = env_var(provider) {
        if let Ok(key) = std::env::var(var) {
            if !key.trim().is_empty() {
                return Ok(key);
            }
        }
    }
    let store = load()?;
    match store.api_keys.get(provider.name()) {
        Some(key) if !key.trim().is_empty() => Ok(key.clone()),
        _ => bail!(
            "no API key stored for {p}. Run `agpx login {p}`{}",
            env_var(provider)
                .map(|v| format!(" or export {v}"))
                .unwrap_or_default(),
            p = provider.name()
        ),
    }
}

pub fn set_api_key(provider: Provider, key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        bail!("empty API key");
    }
    let mut store = load()?;
    store
        .api_keys
        .insert(provider.name().to_string(), key.to_string());
    save(&store)?;
    Ok(())
}

pub fn clear_api_key(provider: Provider) -> Result<bool> {
    let mut store = load()?;
    let removed = store.api_keys.remove(provider.name()).is_some();
    if removed {
        save(&store)?;
    }
    Ok(removed)
}

pub fn has_api_key(provider: Provider) -> bool {
    api_key(provider).is_ok()
}
