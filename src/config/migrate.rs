use super::*;

pub(super) fn read_lines(name: &str) -> Vec<String> {
    config_dir()
        .and_then(|d| std::fs::read_to_string(d.join(name)).ok())
        .map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

pub(super) fn pairs_from(lines: Vec<String>, primary: &str) -> Vec<Pair> {
    lines
        .into_iter()
        .map(|l| match l.split_once('|') {
            Some((d, e)) => Pair { device: d.into(), exe: e.into() },
            None => Pair { device: primary.into(), exe: l },
        })
        .collect()
}

// Builds the first config.toml from the old txt files (or defaults) and archives those files.
pub(super) fn migrate(primary: &str) -> Config {
    let old = ["pinned.txt", "order.txt", "icon_only.txt", "icon_overrides.txt"];
    let have_old = config_dir().map(|d| old.iter().any(|f| d.join(f).exists())).unwrap_or(false);
    let icon_rules: Vec<IconRule> = read_lines("icon_overrides.txt")
        .into_iter()
        .filter_map(|l| {
            let mut p = l.splitn(3, '|');
            Some(IconRule { exe: p.next()?.trim().into(), title: p.next()?.trim().into(), icon: p.next()?.trim().into() })
        })
        .collect();
    let mut cfg = Config {
        pins: pairs_from(read_lines("pinned.txt"), primary),
        order: pairs_from(read_lines("order.txt"), primary),
        icon_only: read_lines("icon_only.txt").into_iter().map(|l| l.to_lowercase()).collect(),
        icon_rules,
        ..Config::default()
    };
    if !have_old {
        cfg.icon_only = vec!["ferdium.exe".into()];
        cfg.icon_rules = ["cmd.exe", "windowsterminal.exe", "powershell.exe", "pwsh.exe"]
            .iter()
            .map(|e| IconRule { exe: (*e).into(), title: "github copilot".into(), icon: "copilot.ico".into() })
            .collect();
    }
    if let Some(d) = config_dir() {
        for f in old {
            let _ = std::fs::rename(d.join(f), d.join(format!("{f}.migrated")));
        }
    }
    cfg
}

