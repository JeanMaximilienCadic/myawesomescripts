//! Persistent desired-state registry of tunnels, stored at
//! `~/.config/awsx2/tunnels.json`. Drives `tunnel-up` and the supervisor.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelKind {
    /// Direct SSM port-forward to an instance matched by name pattern.
    Direct,
    /// Port-forward to `host` via a bastion matched by name pattern.
    Remote,
}

/// A persistent tunnel definition. Deduplicated by `local_port`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelSpec {
    /// Friendly identifier (defaults to host/pattern when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub kind: TunnelKind,
    /// Instance (Direct) or bastion (Remote) Name-tag substring.
    pub pattern: String,
    /// Target host for Remote tunnels; ignored for Direct.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub local_port: u16,
    pub remote_port: u16,
    /// Bind address for the local listener (default "0.0.0.0").
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

fn default_bind() -> String {
    "0.0.0.0".to_string()
}

impl TunnelSpec {
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .or_else(|| self.host.clone())
            .unwrap_or_else(|| self.pattern.clone())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub tunnels: Vec<TunnelSpec>,
}

// ── Filesystem locations ────────────────────────────────────────────────────

pub fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::Registry("cannot determine home directory".into()))?;
    let dir = home.join(".config").join("awsx2");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Registry(format!("cannot create {}: {}", dir.display(), e)))?;
    Ok(dir)
}

pub fn registry_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("tunnels.json"))
}

// ── Load / save ─────────────────────────────────────────────────────────────

pub fn load() -> Result<Vec<TunnelSpec>> {
    let path = registry_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&path)
        .map_err(|e| AppError::Registry(format!("read {}: {}", path.display(), e)))?;
    let reg: Registry = serde_json::from_str(&data)
        .map_err(|e| AppError::Registry(format!("parse {}: {}", path.display(), e)))?;
    Ok(reg.tunnels)
}

pub fn save(specs: &[TunnelSpec]) -> Result<()> {
    let path = registry_path()?;
    let reg = Registry { tunnels: specs.to_vec() };
    let json = serde_json::to_string_pretty(&reg)
        .map_err(|e| AppError::Registry(format!("serialize: {}", e)))?;
    std::fs::write(&path, json)
        .map_err(|e| AppError::Registry(format!("write {}: {}", path.display(), e)))?;
    Ok(())
}

// ── Pure mutation helpers (unit-tested without filesystem) ────────────────────

/// Replace any entry with the same `local_port`, else append.
pub fn upsert_into(specs: &mut Vec<TunnelSpec>, spec: TunnelSpec) {
    if let Some(existing) = specs.iter_mut().find(|s| s.local_port == spec.local_port) {
        *existing = spec;
    } else {
        specs.push(spec);
    }
}

/// Remove entries matching `key` as a local-port number or a display name.
/// Returns true if anything was removed.
pub fn remove_from(specs: &mut Vec<TunnelSpec>, key: &str) -> bool {
    let before = specs.len();
    let as_port: Option<u16> = key.parse().ok();
    specs.retain(|s| {
        let port_match = as_port.is_some_and(|p| p == s.local_port);
        let name_match = s.display_name() == key;
        !(port_match || name_match)
    });
    specs.len() != before
}

// ── Filesystem-backed wrappers ────────────────────────────────────────────────

pub fn upsert(spec: TunnelSpec) -> Result<()> {
    let mut specs = load()?;
    upsert_into(&mut specs, spec);
    save(&specs)
}

pub fn remove(key: &str) -> Result<bool> {
    let mut specs = load()?;
    let removed = remove_from(&mut specs, key);
    if removed {
        save(&specs)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(port: u16, name: &str) -> TunnelSpec {
        TunnelSpec {
            name: Some(name.into()),
            kind: TunnelKind::Direct,
            pattern: format!("{}-ec2", name),
            host: None,
            local_port: port,
            remote_port: 8000,
            bind: "0.0.0.0".into(),
            profile: None,
        }
    }

    #[test]
    fn roundtrip_json() {
        let specs = vec![spec(18000, "dev")];
        let json = serde_json::to_string(&Registry { tunnels: specs.clone() }).unwrap();
        let back: Registry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tunnels.len(), 1);
        assert_eq!(back.tunnels[0].local_port, 18000);
    }

    #[test]
    fn deserialize_minimal_applies_defaults() {
        let json = r#"{"tunnels":[{"kind":"direct","pattern":"x","local_port":1,"remote_port":2}]}"#;
        let reg: Registry = serde_json::from_str(json).unwrap();
        assert_eq!(reg.tunnels[0].bind, "0.0.0.0");
        assert!(reg.tunnels[0].name.is_none());
    }

    #[test]
    fn upsert_replaces_same_port() {
        let mut v = vec![spec(18000, "dev")];
        upsert_into(&mut v, spec(18000, "dev2"));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name.as_deref(), Some("dev2"));
        upsert_into(&mut v, spec(28000, "prod"));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn remove_by_port_or_name() {
        let mut v = vec![spec(18000, "dev"), spec(28000, "prod")];
        assert!(remove_from(&mut v, "18000"));
        assert_eq!(v.len(), 1);
        assert!(remove_from(&mut v, "prod"));
        assert!(v.is_empty());
        assert!(!remove_from(&mut v, "nope"));
    }
}
