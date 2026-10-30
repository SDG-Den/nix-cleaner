use std::collections::HashMap;

use crate::model::PathInfo;

pub struct Graph {
    pub paths: Vec<String>,
    pub nar: Vec<u64>,
    pub refs: Vec<Vec<usize>>,
    pub derivers: Vec<Option<usize>>,
    pub outputs: Vec<Vec<usize>>,
    by_path: HashMap<String, usize>,
}

impl Graph {
    pub fn new(paths: &[PathInfo]) -> Self {
        let by_path: HashMap<String, usize> = paths
            .iter()
            .enumerate()
            .map(|(index, info)| (info.path.clone(), index))
            .collect();

        let refs = paths
            .iter()
            .map(|info| {
                info.references
                    .iter()
                    .filter_map(|reference| by_path.get(reference).copied())
                    .collect()
            })
            .collect();

        let derivers: Vec<Option<usize>> = paths
            .iter()
            .map(|info| {
                info.deriver
                    .as_ref()
                    .and_then(|deriver| by_path.get(deriver).copied())
            })
            .collect();

        let mut outputs: Vec<Vec<usize>> = vec![Vec::new(); paths.len()];
        for (index, deriver) in derivers.iter().enumerate() {
            if let Some(deriver) = deriver {
                outputs[*deriver].push(index);
            }
        }

        Graph {
            paths: paths.iter().map(|info| info.path.clone()).collect(),
            nar: paths.iter().map(|info| info.nar_size).collect(),
            refs,
            derivers,
            outputs,
            by_path,
        }
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn get(&self, path: &str) -> Option<usize> {
        self.by_path.get(path).copied()
    }

    pub fn is_derivation(&self, index: usize) -> bool {
        self.paths[index].ends_with(".drv")
    }

    pub fn closure(&self, start: usize) -> Vec<u32> {
        let mut seen = vec![false; self.len()];
        let mut stack = vec![start];
        let mut out = Vec::new();
        seen[start] = true;
        while let Some(index) = stack.pop() {
            out.push(index as u32);
            for &reference in &self.refs[index] {
                if !seen[reference] {
                    seen[reference] = true;
                    stack.push(reference);
                }
            }
        }
        out
    }

    pub fn closure_size(&self, start: usize) -> u64 {
        self.closure(start)
            .into_iter()
            .map(|index| self.nar[index as usize])
            .sum()
    }

    pub fn referrers(&self) -> Vec<Vec<usize>> {
        let mut reverse = vec![Vec::new(); self.len()];
        for (index, refs) in self.refs.iter().enumerate() {
            for &reference in refs {
                reverse[reference].push(index);
            }
        }
        reverse
    }
}
