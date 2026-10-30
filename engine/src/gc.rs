use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::graph::Graph;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GcPolicy {
    pub keep_derivations: bool,
    pub keep_outputs: bool,
}

impl Default for GcPolicy {
    fn default() -> Self {
        GcPolicy {
            keep_derivations: true,
            keep_outputs: false,
        }
    }
}

pub fn load() -> GcPolicy {
    GcPolicy {
        keep_derivations: config_bool("keep-derivations", true),
        keep_outputs: config_bool("keep-outputs", false),
    }
}

fn config_bool(name: &str, fallback: bool) -> bool {
    let output = Command::new("nix")
        .args([
            "--extra-experimental-features",
            "nix-command",
            "config",
            "show",
            name,
        ])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            match String::from_utf8_lossy(&output.stdout).trim() {
                "true" => true,
                "false" => false,
                _ => fallback,
            }
        }
        _ => fallback,
    }
}

pub fn closure(graph: &Graph, start: usize, policy: &GcPolicy) -> Vec<u32> {
    let mut seen = vec![false; graph.len()];
    let mut stack = vec![start];
    let mut out = Vec::new();
    seen[start] = true;

    while let Some(index) = stack.pop() {
        out.push(index as u32);

        for &reference in &graph.refs[index] {
            if !seen[reference] {
                seen[reference] = true;
                stack.push(reference);
            }
        }

        if policy.keep_derivations {
            if let Some(deriver) = graph.derivers[index] {
                if !seen[deriver] {
                    seen[deriver] = true;
                    stack.push(deriver);
                }
            }
        }

        if policy.keep_outputs {
            for &output in &graph.outputs[index] {
                if !seen[output] {
                    seen[output] = true;
                    stack.push(output);
                }
            }
        }
    }

    out
}
