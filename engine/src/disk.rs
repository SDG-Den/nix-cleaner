use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::gc::{self, GcPolicy};
use crate::graph::Graph;

#[derive(Debug, Clone, Copy, Default)]
pub struct DiskUsage {
    pub bytes: u64,
    pub shared_bytes: u64,
    pub files: u64,
}

pub fn closure_usage(
    graph: &Graph,
    start: usize,
    policy: &GcPolicy,
    progress: &mut dyn FnMut(u32, u32, String),
) -> DiskUsage {
    let closure = gc::closure(graph, start, policy);
    let total = closure.len() as u32;
    let mut inodes: HashMap<(u64, u64), (u64, u64)> = HashMap::new();
    let mut files = 0u64;

    for (position, path_index) in closure.iter().enumerate() {
        let path = &graph.paths[*path_index as usize];
        walk(Path::new(path), &mut inodes, &mut files);
        progress(position as u32 + 1, total, path.clone());
    }

    let mut bytes = 0u64;
    let mut shared_bytes = 0u64;
    for (size, links) in inodes.values() {
        bytes += size;
        if *links > 1 {
            shared_bytes += size;
        }
    }

    DiskUsage {
        bytes,
        shared_bytes,
        files,
    }
}

fn walk(root: &Path, inodes: &mut HashMap<(u64, u64), (u64, u64)>, files: &mut u64) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.flatten() {
                    stack.push(entry.path());
                }
            }
            continue;
        }
        let size = metadata.blocks() * 512;
        inodes
            .entry((metadata.dev(), metadata.ino()))
            .or_insert((size, metadata.nlink()));
        *files += 1;
    }
}
