use std::collections::HashSet;
use std::process::Command;

use crate::graph::Graph;
use crate::model::Index;

#[derive(Debug, Clone, Default)]
pub struct Validation {
    pub store_paths: usize,
    pub live_paths: usize,
    pub dead_paths: usize,
    pub live_bytes: u64,
    pub dead_bytes: u64,
    pub df_used: u64,
    pub nix_dead_paths: usize,
    pub nix_dead_bytes: u64,
    pub nix_dead_available: bool,
}

pub fn run(index: &Index, graph: &Graph) -> Validation {
    let mut validation = Validation {
        store_paths: index.paths.len(),
        ..Default::default()
    };

    for (path_index, owners) in index.analysis.path_roots.iter().enumerate() {
        if owners.is_empty() {
            validation.dead_paths += 1;
            validation.dead_bytes += graph.nar[path_index];
        } else {
            validation.live_paths += 1;
            validation.live_bytes += graph.nar[path_index];
        }
    }

    validation.df_used = df_used();
    if let Some(dead) = nix_dead_paths() {
        validation.nix_dead_available = true;
        validation.nix_dead_paths = dead.len();
        validation.nix_dead_bytes = dead
            .iter()
            .filter_map(|path| graph.get(path))
            .map(|path_index| graph.nar[path_index])
            .sum();
    }

    validation
}

pub fn df_used() -> u64 {
    let output = Command::new("df")
        .args(["-B1", "--output=used", "/nix/store"])
        .output();
    let Ok(output) = output else {
        return 0;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .skip(1)
        .find_map(|line| line.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

pub fn nix_dead_paths() -> Option<HashSet<String>> {
    let output = Command::new("nix-store")
        .args(["--gc", "--print-dead"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(
        text.lines()
            .map(str::trim)
            .filter(|line| line.starts_with("/nix/store/"))
            .map(str::to_string)
            .collect(),
    )
}
