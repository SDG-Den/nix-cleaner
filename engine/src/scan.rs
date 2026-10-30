use std::collections::HashMap;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::attribution;
use crate::cache;
use crate::gc;
use crate::graph::Graph;
use crate::model::{Index, PathInfo};
use crate::progress::{self, ScanEvent};
use crate::roots;

pub const NIX_ARGS: [&str; 2] = ["--extra-experimental-features", "nix-command"];

#[derive(Deserialize)]
struct RawPathInfo {
    #[serde(rename = "narSize", default)]
    nar_size: u64,
    #[serde(default)]
    references: Vec<String>,
    #[serde(default)]
    deriver: Option<String>,
    #[serde(rename = "registrationTime", default)]
    registration_time: i64,
}

pub fn store_paths(report: &mut dyn FnMut(ScanEvent)) -> Result<Vec<PathInfo>> {
    progress::stage(report, "store");
    progress::log(report, "reading store metadata with nix path-info --all");

    let output = Command::new("nix")
        .args(NIX_ARGS)
        .args(["path-info", "--all", "--json", "--json-format", "1"])
        .output()
        .context("running nix path-info, is nix on PATH?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "nix path-info failed with {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    let raw: HashMap<String, RawPathInfo> =
        serde_json::from_slice(&output.stdout).context("parsing nix path-info output")?;

    let mut paths: Vec<PathInfo> = raw
        .into_iter()
        .map(|(path, info)| PathInfo {
            path,
            nar_size: info.nar_size,
            references: info.references,
            deriver: info.deriver,
            registration_time: info.registration_time,
        })
        .collect();
    paths.sort_by(|left, right| left.path.cmp(&right.path));

    progress::log(report, format!("{} store paths registered", paths.len()));
    Ok(paths)
}

pub fn scan(report: &mut dyn FnMut(ScanEvent)) -> Result<Index> {
    let paths = store_paths(report)?;

    progress::stage(report, "roots");
    let home = cache::home_dir();
    let discovery = roots::discover(&home);
    for warning in &discovery.warnings {
        progress::log(report, warning.clone());
    }
    progress::log(report, format!("{} gc roots discovered", discovery.roots.len()));

    progress::stage(report, "graph");
    let graph = Graph::new(&paths);
    progress::log(
        report,
        format!(
            "dependency graph built with {} edges",
            graph.refs.iter().map(Vec::len).sum::<usize>()
        ),
    );

    let policy = gc::load();
    progress::log(
        report,
        format!(
            "gc policy: keep-derivations={}, keep-outputs={}",
            policy.keep_derivations, policy.keep_outputs
        ),
    );

    progress::stage(report, "closures");
    let mut tick = |done: u32, total: u32| {
        progress::progress(
            &mut *report,
            "closures",
            done,
            total,
            "computing root closures".to_string(),
        );
    };
    let analysis =
        attribution::analyze_with_progress(&graph, &discovery.roots, &policy, &mut tick);

    progress::stage(report, "cache");
    let live_bytes: u64 = analysis
        .path_roots
        .iter()
        .enumerate()
        .filter(|(_, owners)| !owners.is_empty())
        .map(|(index, _)| graph.nar[index])
        .sum();

    let index = Index {
        cache_version: cache::VERSION,
        scanned_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0),
        paths,
        roots: discovery.roots,
        analysis,
        live_bytes,
        policy,
        warnings: discovery.warnings,
    };

    let written = cache::save(&index)?;
    progress::log(report, format!("index cached at {}", written.display()));
    Ok(index)
}

pub fn scan_and_report(report: &mut dyn FnMut(ScanEvent)) {
    match scan(report) {
        Ok(index) => report(ScanEvent::Done(Box::new(index))),
        Err(error) => report(ScanEvent::Failed(format!("{error:#}"))),
    }
}
