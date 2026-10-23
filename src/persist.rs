use std::os::unix::fs::chown;
use std::path::PathBuf;
use std::{env, fs};

use crate::curve::Curve;

/// The invoking user's config directory — under `sudo` that is their home, not root's.
fn config_dir() -> Option<PathBuf> {
    let sudo_home = env::var("SUDO_USER")
        .ok()
        .filter(|u| !u.is_empty() && u != "root")
        .map(|u| PathBuf::from("/Users").join(u))
        .filter(|p| p.is_dir());
    let home = sudo_home.or_else(|| env::var_os("HOME").map(PathBuf::from))?;
    Some(home.join(".config").join("macfan"))
}

fn sudo_owner() -> Option<(u32, u32)> {
    let uid = env::var("SUDO_UID").ok()?.parse().ok()?;
    let gid = env::var("SUDO_GID").ok()?.parse().ok()?;
    Some((uid, gid))
}

pub fn load() -> Option<Curve> {
    let text = fs::read_to_string(config_dir()?.join("curve")).ok()?;
    Curve::parse(text.trim()).ok()
}

/// Best-effort: a failure to persist must never disturb fan control.
pub fn save(curve: &Curve) {
    let Some(dir) = config_dir() else { return };
    let created = !dir.exists();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file = dir.join("curve");
    if fs::write(&file, format!("{}\n", curve.to_input_string())).is_err() {
        return;
    }
    // Hand files created by `sudo macfan` back to the real user.
    if let Some((uid, gid)) = sudo_owner() {
        if created {
            let _ = chown(&dir, Some(uid), Some(gid));
        }
        let _ = chown(&file, Some(uid), Some(gid));
    }
}
