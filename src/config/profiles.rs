use super::*;

// Everything a profile stores; per-run window bookkeeping is left out.
pub(super) fn profile_text(cfg: &Config) -> String {
    let mut c = cfg.clone();
    c.windows.clear();
    c.order.clear();
    toml::to_string_pretty(&c).unwrap_or_default()
}

pub(super) fn profiles_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("profiles"))
}

pub(crate) fn clean_profile_name(s: &str) -> String {
    let cleaned: String = s.trim().chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    cleaned.trim().chars().take(40).collect()
}

pub(crate) fn list_profiles() -> Vec<String> {
    let Some(dir) = profiles_dir() else { return Vec::new() };
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension().is_some_and(|x| x == "toml")).then(|| p.file_stem().map(|s| s.to_string_lossy().into_owned())).flatten()
        })
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v
}

// Writes a file and logs the failure, if any.
pub(super) fn write_logged(path: &std::path::Path, text: String) -> bool {
    match std::fs::write(path, text) {
        Ok(()) => true,
        Err(e) => {
            log(&format!("could not write {}: {e}", path.display()));
            false
        }
    }
}

pub(crate) fn save_profile(name: &str) -> bool {
    let name = clean_profile_name(name);
    let Some(dir) = profiles_dir() else { return false };
    if name.is_empty() || std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let text = with_store(|st| profile_text(&st.cfg));
    write_logged(&dir.join(format!("{name}.toml")), text)
}

pub(crate) fn delete_profile(name: &str) {
    if let Some(dir) = profiles_dir() {
        let _ = std::fs::remove_file(dir.join(format!("{}.toml", clean_profile_name(name))));
    }
}

// The saved profile identical to the current settings, if any.
pub(crate) fn active_profile() -> Option<String> {
    let now = with_store(|st| profile_text(&st.cfg));
    let dir = profiles_dir()?;
    list_profiles().into_iter().find(|n| {
        std::fs::read_to_string(dir.join(format!("{n}.toml")))
            .ok()
            .and_then(|t| toml::from_str::<Config>(&t).ok())
            .is_some_and(|c| profile_text(&c) == now)
    })
}

pub(super) fn apply_profile_text(text: &str) -> bool {
    let Ok(new) = toml::from_str::<Config>(text) else { return false };
    mutate(|c| {
        let (windows, order) = (std::mem::take(&mut c.windows), std::mem::take(&mut c.order));
        *c = new;
        c.windows = windows;
        c.order = order;
    });
    true
}

pub(crate) fn load_profile(name: &str) -> bool {
    let Some(dir) = profiles_dir() else { return false };
    std::fs::read_to_string(dir.join(format!("{}.toml", clean_profile_name(name)))).is_ok_and(|t| apply_profile_text(&t))
}

pub(crate) fn export_settings(path: &std::path::Path) -> bool {
    let text = with_store(|st| profile_text(&st.cfg));
    write_logged(path, text)
}

// Adds a settings file to the profile list (named after the file) without applying it.
pub(crate) fn import_profile(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let cfg = toml::from_str::<Config>(&text).ok()?;
    let name = clean_profile_name(&path.file_stem()?.to_string_lossy());
    let dir = profiles_dir()?;
    if name.is_empty() {
        return None;
    }
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join(format!("{name}.toml")), profile_text(&cfg)).ok()?;
    Some(name)
}

