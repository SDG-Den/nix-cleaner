use crate::gc::{self, GcPolicy};
use crate::graph::Graph;
use crate::model::{Analysis, Root};

pub fn analyze(graph: &Graph, roots: &[Root]) -> Analysis {
    analyze_with_progress(graph, roots, &GcPolicy::default(), &mut |_, _| {})
}

pub fn analyze_with_progress(
    graph: &Graph,
    roots: &[Root],
    policy: &GcPolicy,
    progress: &mut dyn FnMut(u32, u32),
) -> Analysis {
    let mut analysis = Analysis {
        closure_sizes: vec![0; roots.len()],
        individual_sizes: vec![0; roots.len()],
        path_roots: vec![Vec::new(); graph.len()],
    };

    for (root_index, root) in roots.iter().enumerate() {
        if let Some(start) = graph.get(&root.target) {
            let closure = gc::closure(graph, start, policy);
            let mut total = 0u64;
            for path_index in closure {
                total += graph.nar[path_index as usize];
                analysis.path_roots[path_index as usize].push(root_index as u32);
            }
            analysis.closure_sizes[root_index] = total;
        }
        progress(root_index as u32 + 1, roots.len() as u32);
    }

    for (path_index, owners) in analysis.path_roots.iter().enumerate() {
        if owners.len() == 1 {
            analysis.individual_sizes[owners[0] as usize] += graph.nar[path_index];
        }
    }

    analysis
}

pub fn selection_savings(graph: &Graph, analysis: &Analysis, selection: &[u32]) -> u64 {
    if selection.is_empty() {
        return 0;
    }
    let mut selected = selection.to_vec();
    selected.sort_unstable();
    selected.dedup();

    let mut total = 0u64;
    for (path_index, owners) in analysis.path_roots.iter().enumerate() {
        if owners.is_empty() {
            continue;
        }
        if owners
            .iter()
            .all(|owner| selected.binary_search(owner).is_ok())
        {
            total += graph.nar[path_index];
        }
    }
    total
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub path: String,
    pub own: u64,
    pub cumulative: u64,
    pub shared: bool,
    pub children: Vec<TreeNode>,
}

pub fn build_tree(graph: &Graph, root: &Root, policy: &GcPolicy) -> Option<TreeNode> {
    let start = graph.get(&root.target)?;
    let mut counted = vec![false; graph.len()];
    Some(build_node(graph, start, policy, &mut counted))
}

fn children_of(graph: &Graph, index: usize, policy: &GcPolicy) -> Vec<usize> {
    let mut children = graph.refs[index].clone();
    if policy.keep_derivations {
        if let Some(deriver) = graph.derivers[index] {
            children.push(deriver);
        }
    }
    if policy.keep_outputs {
        children.extend_from_slice(&graph.outputs[index]);
    }
    children.sort_unstable();
    children.dedup();
    children
}

fn build_node(graph: &Graph, index: usize, policy: &GcPolicy, counted: &mut [bool]) -> TreeNode {
    let own = graph.nar[index];
    if counted[index] {
        return TreeNode {
            path: graph.paths[index].clone(),
            own,
            cumulative: 0,
            shared: true,
            children: Vec::new(),
        };
    }
    counted[index] = true;

    let mut children: Vec<TreeNode> = children_of(graph, index, policy)
        .into_iter()
        .map(|child| build_node(graph, child, policy, counted))
        .collect();
    children.sort_by(|left, right| {
        right
            .cumulative
            .cmp(&left.cumulative)
            .then_with(|| left.path.cmp(&right.path))
    });

    let cumulative = own + children.iter().map(|child| child.cumulative).sum::<u64>();
    TreeNode {
        path: graph.paths[index].clone(),
        own,
        cumulative,
        shared: false,
        children,
    }
}

pub fn flatten(tree: &TreeNode, depth: u32, out: &mut Vec<(u32, TreeNode)>, max_depth: u32) {
    out.push((depth, tree.clone()));
    if depth >= max_depth {
        return;
    }
    for child in &tree.children {
        flatten(child, depth + 1, out, max_depth);
    }
}
