use engine::attribution::{analyze, build_tree, selection_savings};
use engine::graph::Graph;
use engine::model::{PathInfo, Protection, Root, RootKind};

fn path(name: &str, nar_size: u64, references: &[&str]) -> PathInfo {
    PathInfo {
        path: name.to_string(),
        nar_size,
        references: references.iter().map(|value| value.to_string()).collect(),
        deriver: None,
        registration_time: 0,
    }
}

fn root(link: &str, target: &str) -> Root {
    Root {
        link: link.to_string(),
        target: target.to_string(),
        kind: RootKind::SystemGeneration,
        label: link.to_string(),
        generation: None,
        protection: Protection::None,
    }
}

fn diamond() -> Graph {
    Graph::new(&[
        path("/nix/store/a", 10, &["/nix/store/b", "/nix/store/c"]),
        path("/nix/store/b", 20, &["/nix/store/d"]),
        path("/nix/store/c", 30, &["/nix/store/d"]),
        path("/nix/store/d", 40, &[]),
    ])
}

#[test]
fn closure_sizes_follow_references() {
    let graph = diamond();
    let start = graph.get("/nix/store/a").unwrap();
    assert_eq!(graph.closure_size(start), 100);
}

#[test]
fn individual_sizes_count_only_exclusive_paths() {
    let graph = diamond();
    let roots = vec![
        root("r1", "/nix/store/a"),
        root("r2", "/nix/store/d"),
    ];
    let analysis = analyze(&graph, &roots);
    assert_eq!(analysis.closure_sizes, vec![100, 40]);
    assert_eq!(analysis.individual_sizes, vec![60, 0]);
}

#[test]
fn selection_savings_respect_shared_dependencies() {
    let graph = diamond();
    let roots = vec![root("r1", "/nix/store/a"), root("r2", "/nix/store/d")];
    let analysis = analyze(&graph, &roots);

    assert_eq!(selection_savings(&graph, &analysis, &[0]), 60);
    assert_eq!(selection_savings(&graph, &analysis, &[1]), 0);
    assert_eq!(selection_savings(&graph, &analysis, &[0, 1]), 100);
    assert_eq!(selection_savings(&graph, &analysis, &[]), 0);
}

#[test]
fn tree_totals_match_closure_and_mark_repeats() {
    let graph = diamond();
    let entry = root("r1", "/nix/store/a");
    let tree = build_tree(&graph, &entry, &engine::gc::GcPolicy::default()).unwrap();
    assert_eq!(tree.cumulative, 100);

    let shared: Vec<_> = tree
        .children
        .iter()
        .flat_map(|child| child.children.iter())
        .filter(|child| child.shared)
        .collect();
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].cumulative, 0);
}

#[test]
fn roots_outside_the_store_are_ignored() {
    let graph = diamond();
    let analysis = analyze(&graph, &[root("missing", "/nix/store/does-not-exist")]);
    assert_eq!(analysis.closure_sizes, vec![0]);
    assert_eq!(analysis.individual_sizes, vec![0]);
}

#[test]
fn keep_derivations_pulls_in_derivers_and_their_inputs() {
    use engine::attribution::analyze_with_progress;
    use engine::gc::GcPolicy;

    let paths = vec![
        PathInfo {
            path: "/nix/store/output".to_string(),
            nar_size: 10,
            references: Vec::new(),
            deriver: Some("/nix/store/build.drv".to_string()),
            registration_time: 0,
        },
        PathInfo {
            path: "/nix/store/build.drv".to_string(),
            nar_size: 5,
            references: vec!["/nix/store/source".to_string()],
            deriver: None,
            registration_time: 0,
        },
        PathInfo {
            path: "/nix/store/source".to_string(),
            nar_size: 7,
            references: Vec::new(),
            deriver: None,
            registration_time: 0,
        },
    ];
    let graph = Graph::new(&paths);
    let roots = vec![root("r1", "/nix/store/output")];

    let kept = analyze_with_progress(
        &graph,
        &roots,
        &GcPolicy {
            keep_derivations: true,
            keep_outputs: false,
        },
        &mut |_, _| {},
    );
    assert_eq!(kept.closure_sizes, vec![22]);

    let dropped = analyze_with_progress(
        &graph,
        &roots,
        &GcPolicy {
            keep_derivations: false,
            keep_outputs: false,
        },
        &mut |_, _| {},
    );
    assert_eq!(dropped.closure_sizes, vec![10]);
}
