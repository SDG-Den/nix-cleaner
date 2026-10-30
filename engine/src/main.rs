use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use engine::attribution::{self, build_tree, selection_savings};
use engine::cache;
use engine::disk;
use engine::graph::Graph;
use engine::model::Index;
use engine::progress::ScanEvent;
use engine::{scan, validate};

#[derive(Parser)]
#[command(name = "nix-cleaner-engine", about = "Scan engine for nix-cleaner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Scan,
    Roots,
    Tree { root: usize },
    Disk { root: usize },
    Validate,
    CacheInfo,
    Dead,
    Live,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Scan => {
            let index = scan::scan(&mut print_event)?;
            println!(
                "{} paths, {} roots, {:.2} GiB live",
                index.paths.len(),
                index.roots.len(),
                index.live_bytes as f64 / 1024.0 / 1024.0 / 1024.0
            );
        }
        Command::Roots => {
            let index = load_or_scan()?;
            for (position, root) in index.roots.iter().enumerate() {
                println!(
                    "{position:>3}  {:<18} {:<34} {:>10}  {:>10}  {}",
                    format!("{:?}", root.kind),
                    root.label,
                    format_bytes(index.analysis.closure_sizes[position]),
                    format_bytes(index.analysis.individual_sizes[position]),
                    if root.protection.is_protected() {
                        format!("protected: {}", root.protection.reason())
                    } else {
                        String::new()
                    }
                );
            }
        }
        Command::Tree { root } => {
            let index = load_or_scan()?;
            let graph = Graph::new(&index.paths);
            let entry = index
                .roots
                .get(root)
                .ok_or_else(|| anyhow!("root index out of range"))?;
            let tree = build_tree(&graph, entry, &index.policy)
                .ok_or_else(|| anyhow!("root not in store"))?;
            println!("{}", serde_json::to_string_pretty(&tree_to_json(&tree))?);
        }
        Command::Disk { root } => {
            let index = load_or_scan()?;
            let graph = Graph::new(&index.paths);
            let entry = index
                .roots
                .get(root)
                .ok_or_else(|| anyhow!("root index out of range"))?;
            let start = graph
                .get(&entry.target)
                .ok_or_else(|| anyhow!("root not in store"))?;
            let policy = engine::gc::load();
            let usage = disk::closure_usage(&graph, start, &policy, &mut |done, total, path| {
                eprintln!("{done}/{total} {path}");
            });
            println!(
                "bytes={} shared={} files={}",
                usage.bytes, usage.shared_bytes, usage.files
            );
        }
        Command::Validate => {
            let index = load_or_scan()?;
            let graph = Graph::new(&index.paths);
            let result = validate::run(&index, &graph);
            println!("{result:#?}");
        }
        Command::Dead => {
            let index = load_or_scan()?;
            for (position, owners) in index.analysis.path_roots.iter().enumerate() {
                if owners.is_empty() {
                    println!("{}", index.paths[position].path);
                }
            }
        }
        Command::Live => {
            let index = load_or_scan()?;
            for (position, owners) in index.analysis.path_roots.iter().enumerate() {
                if !owners.is_empty() {
                    println!("{}", index.paths[position].path);
                }
            }
        }
        Command::CacheInfo => {
            let path = cache::cache_path();
            let metadata = std::fs::metadata(&path)?;
            println!("{} ({})", path.display(), format_bytes(metadata.len()));
            let index = cache::load()?.ok_or_else(|| anyhow!("no cached index"))?;
            println!(
                "scanned_at={} paths={} roots={}",
                index.scanned_at,
                index.paths.len(),
                index.roots.len()
            );
            let selection: Vec<u32> = (0..index.roots.len() as u32).collect();
            let graph = Graph::new(&index.paths);
            println!(
                "everything={}",
                format_bytes(selection_savings(&graph, &index.analysis, &selection))
            );
        }
    }
    Ok(())
}

fn load_or_scan() -> Result<Index> {
    if let Some(index) = cache::load()? {
        eprintln!("using cached index from {}", index.scanned_at);
        return Ok(index);
    }
    eprintln!("no cached index, scanning");
    scan::scan(&mut print_event)
}

fn print_event(event: ScanEvent) {
    match event {
        ScanEvent::Stage { name } => eprintln!("[stage] {name}"),
        ScanEvent::Progress {
            stage,
            done,
            total,
            message,
        } => eprintln!("[{stage}] {done}/{total} {message}"),
        ScanEvent::Log(line) => eprintln!("[log] {line}"),
        ScanEvent::Done(_) => {}
        ScanEvent::Failed(error) => eprintln!("[failed] {error}"),
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn tree_to_json(node: &attribution::TreeNode) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert("path".into(), node.path.clone().into());
    object.insert("own".into(), node.own.into());
    object.insert("cumulative".into(), node.cumulative.into());
    object.insert("shared".into(), node.shared.into());
    object.insert(
        "children".into(),
        node.children.iter().map(tree_to_json).collect::<Vec<_>>().into(),
    );
    serde_json::Value::Object(object)
}
