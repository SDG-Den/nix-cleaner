use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{Protection, Root, RootKind};

pub struct Discovery {
    pub roots: Vec<Root>,
    pub warnings: Vec<String>,
}

pub fn discover(home: &Path) -> Discovery {
    let mut discovery = Discovery {
        roots: Vec::new(),
        warnings: Vec::new(),
    };

    scan_generations(
        &mut discovery,
        Path::new("/nix/var/nix/profiles"),
        "system",
        "System",
        RootKind::SystemCurrent,
        RootKind::SystemGeneration,
    );
    scan_per_user(&mut discovery, Path::new("/nix/var/nix/profiles/per-user"));
    scan_home_manager(&mut discovery, home);
    scan_home_manager_gcroots(&mut discovery, &home.join(".local/state/home-manager/gcroots"));
    scan_direct_gcroots(&mut discovery, Path::new("/nix/var/nix/gcroots"));
    scan_per_user_gcroots(&mut discovery, Path::new("/nix/var/nix/gcroots/per-user"));
    scan_auto_roots(&mut discovery, Path::new("/nix/var/nix/gcroots/auto"), home);

    apply_protection(&mut discovery.roots);
    dedupe(&mut discovery.roots);
    sort(&mut discovery.roots);
    discovery
}

fn add_root(
    discovery: &mut Discovery,
    link: &Path,
    kind: RootKind,
    label: String,
    generation: Option<u32>,
) {
    let resolved = match fs::canonicalize(link) {
        Ok(resolved) => resolved,
        Err(_) => {
            discovery
                .warnings
                .push(format!("unreadable root: {}", link.display()));
            return;
        }
    };
    let Some(target) = resolved.to_str() else {
        return;
    };
    if !target.starts_with("/nix/store/") {
        discovery.warnings.push(format!(
            "root outside the store ignored: {} -> {}",
            link.display(),
            target
        ));
        return;
    }
    discovery.roots.push(Root {
        link: link.display().to_string(),
        target: target.to_string(),
        kind,
        label,
        generation,
        protection: Protection::None,
    });
}

fn scan_generations(
    discovery: &mut Discovery,
    dir: &Path,
    prefix: &str,
    noun: &str,
    current_kind: RootKind,
    generation_kind: RootKind,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        if name == prefix {
            add_root(
                discovery,
                &path,
                current_kind,
                format!("Current {noun}"),
                None,
            );
        } else if name.starts_with(&format!("{prefix}-")) {
            if let Some(generation) = parse_generation(&name) {
                add_root(
                    discovery,
                    &path,
                    generation_kind,
                    format!("{noun} Generation {generation}"),
                    Some(generation),
                );
            }
        }
    }
}

fn scan_per_user(discovery: &mut Discovery, dir: &Path) {
    let Ok(users) = fs::read_dir(dir) else {
        return;
    };
    for user in users.flatten() {
        if !user.path().is_dir() {
            continue;
        }
        let user_name = user.file_name().to_string_lossy().to_string();
        let Ok(entries) = fs::read_dir(user.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if name == "profile" {
                add_root(
                    discovery,
                    &path,
                    RootKind::Profile,
                    format!("User Profile ({user_name})"),
                    None,
                );
            } else if name.starts_with("profile-") {
                if let Some(generation) = parse_generation(&name) {
                    add_root(
                        discovery,
                        &path,
                        RootKind::Profile,
                        format!("User Profile ({user_name}) Generation {generation}"),
                        Some(generation),
                    );
                }
            }
        }
    }
}

fn scan_home_manager(discovery: &mut Discovery, home: &Path) {
    let generations = home.join(".local/state/nix/profiles/home-manager");
    if generations.is_dir() {
        scan_generations(
            discovery,
            &generations,
            "home-manager",
            "Home Manager",
            RootKind::HomeManagerCurrent,
            RootKind::HomeManagerGeneration,
        );
    }
    let profile = home.join(".local/state/nix/profiles/profile");
    if profile.exists() {
        add_root(
            discovery,
            &profile,
            RootKind::Profile,
            "Home Manager Profile".to_string(),
            None,
        );
    }
    let legacy = home.join(".nix-profile");
    if legacy.exists() {
        add_root(
            discovery,
            &legacy,
            RootKind::Profile,
            "User Profile".to_string(),
            None,
        );
    }
}

fn scan_home_manager_gcroots(discovery: &mut Discovery, dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        if name == "current-home" || name == "home-manager" {
            add_root(
                discovery,
                &path,
                RootKind::HomeManagerCurrent,
                "Current Home Manager".to_string(),
                None,
            );
        } else if let Some(generation) = parse_generation(&name) {
            add_root(
                discovery,
                &path,
                RootKind::HomeManagerGeneration,
                format!("Home Manager Generation {generation}"),
                Some(generation),
            );
        }
    }
}

fn scan_direct_gcroots(discovery: &mut Discovery, dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "auto" || name == "per-user" {
            continue;
        }
        let path = entry.path();
        let (kind, label) = match name.as_str() {
            "current-system" => (RootKind::SystemCurrent, "Current System".to_string()),
            "booted-system" => (RootKind::BootedSystem, "Booted System".to_string()),
            _ => (
                RootKind::Gcroot,
                format!("GC root · {}", entry.file_name().to_string_lossy()),
            ),
        };
        add_root(discovery, &path, kind, label, None);
    }
}

fn scan_per_user_gcroots(discovery: &mut Discovery, dir: &Path) {
    let Ok(users) = fs::read_dir(dir) else {
        return;
    };
    for user in users.flatten() {
        let Ok(entries) = fs::read_dir(user.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            add_root(
                discovery,
                &entry.path(),
                RootKind::Gcroot,
                format!("GC root · {name}"),
                None,
            );
        }
    }
}

fn scan_auto_roots(discovery: &mut Discovery, dir: &Path, home: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(destination) = fs::read_link(entry.path()) else {
            continue;
        };
        if !destination.is_absolute() {
            continue;
        }
        let destination_string = destination.display().to_string();
        if destination_string.starts_with("/nix/var/nix/profiles")
            || destination_string.starts_with("/nix/var/nix/gcroots")
            || destination_string.starts_with("/run/")
        {
            continue;
        }
        let file_name = destination
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let (kind, label) = if file_name == "result" || file_name.starts_with("result-") {
            let parent = destination.parent().unwrap_or(Path::new("/"));
            (
                RootKind::ResultLink,
                format!("{file_name} · {}", shorten(home, parent)),
            )
        } else {
            (
                RootKind::Gcroot,
                format!("GC root · {}", shorten(home, &destination)),
            )
        };
        add_root(discovery, &destination, kind, label, None);
    }
}

pub fn apply_protection(roots: &mut [Root]) {
    let current_system = roots
        .iter()
        .find(|root| root.kind == RootKind::SystemCurrent)
        .map(|root| root.target.clone());
    let booted_system = roots
        .iter()
        .find(|root| root.kind == RootKind::BootedSystem)
        .map(|root| root.target.clone());
    let home_manager_current = roots
        .iter()
        .find(|root| root.kind == RootKind::HomeManagerCurrent)
        .map(|root| root.target.clone());

    for root in roots.iter_mut() {
        match root.kind {
            RootKind::SystemCurrent => root.protection = Protection::CurrentSystem,
            RootKind::BootedSystem => root.protection = Protection::BootedSystem,
            RootKind::HomeManagerCurrent | RootKind::Profile => {
                root.protection = Protection::CurrentProfile
            }
            _ => {}
        }
        if root.link.starts_with("/nix/var/nix/gcroots/") {
            root.protection = Protection::SystemOwned;
        }
    }

    protect_generation_set(
        roots,
        RootKind::SystemGeneration,
        current_system.as_deref(),
        booted_system.as_deref(),
    );
    protect_generation_set(
        roots,
        RootKind::HomeManagerGeneration,
        home_manager_current.as_deref(),
        None,
    );
}

const KEEP_NEWEST: usize = 3;

fn protect_generation_set(
    roots: &mut [Root],
    kind: RootKind,
    current: Option<&str>,
    booted: Option<&str>,
) {
    let mut generations: Vec<(usize, u32)> = roots
        .iter()
        .enumerate()
        .filter(|(_, root)| root.kind == kind)
        .filter_map(|(index, root)| root.generation.map(|generation| (index, generation)))
        .collect();
    generations.sort_by_key(|(_, generation)| *generation);

    let total = generations.len();
    for (position, (index, _)) in generations.iter().enumerate() {
        let target = roots[*index].target.as_str();
        if current.is_some_and(|current| current == target)
            || booted.is_some_and(|booted| booted == target)
        {
            roots[*index].protection = Protection::CurrentGeneration;
            continue;
        }
        if position + KEEP_NEWEST >= total {
            roots[*index].protection = Protection::NewestGeneration;
        }
    }
}

fn dedupe(roots: &mut Vec<Root>) {
    let mut seen_links = std::collections::HashSet::new();
    let mut seen_targets = std::collections::HashSet::new();
    roots.retain(|root| {
        if !seen_links.insert(root.link.clone()) {
            return false;
        }
        let key = if root.kind == RootKind::ResultLink {
            root.link.clone()
        } else {
            root.target.clone()
        };
        seen_targets.insert((root.kind, key))
    });
}

fn sort(roots: &mut [Root]) {
    roots.sort_by(|left, right| {
        left.kind
            .rank()
            .cmp(&right.kind.rank())
            .then_with(|| right.generation.cmp(&left.generation))
            .then_with(|| left.label.cmp(&right.label))
    });
}

fn parse_generation(name: &str) -> Option<u32> {
    let trimmed = name.strip_suffix("-link")?;
    let split = trimmed.rfind('-')?;
    trimmed[split + 1..].parse().ok()
}

pub fn shorten(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

pub fn link_parent(link: &str) -> PathBuf {
    Path::new(link)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/"))
}
