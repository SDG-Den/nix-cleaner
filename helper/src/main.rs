use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::os::unix::io::AsRawFd;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{anyhow, bail, Context, Result};
use engine::model::Protection;
use serde::Deserialize;
use serde_json::{json, Value};

const PLAN_VERSION: u32 = 1;
const CHILD_PATH: &str =
    "/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/run/wrappers/bin";
const GC_LOCK: &str = "/nix/var/nix/gc.lock";
const AUTO_ROOTS: &str = "/nix/var/nix/gcroots/auto";
const STAGING_SUFFIX: &str = ".nix-cleaner-staging";
const ALLOWED_PREFIXES: [&str; 2] = ["/nix/var/nix/profiles/", "/nix/var/nix/gcroots/per-user/"];
const SIZE_BATCH: usize = 500;

fn main() -> std::process::ExitCode {
    let (payload, success) = match execute() {
        Ok(value) => (value.to_string(), true),
        Err(failure) => (failure.render(), false),
    };
    println!("{payload}");
    std::process::ExitCode::from(if success { 0 } else { 1 })
}

struct Failure {
    error: String,
    messages: Vec<String>,
}

impl Failure {
    fn render(&self) -> String {
        json!({
            "ok": false,
            "error": self.error,
            "messages": self.messages,
        })
        .to_string()
    }
}

fn execute() -> std::result::Result<Value, Failure> {
    let mut messages = Vec::new();
    match run(&mut messages) {
        Ok(value) => Ok(value),
        Err(error) => Err(Failure {
            error: format!("{error:#}"),
            messages,
        }),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    DryRun,
    Clean,
}

impl Mode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "dry-run" => Ok(Mode::DryRun),
            "clean" => Ok(Mode::Clean),
            other => bail!("unknown mode: {other}"),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Mode::DryRun => "dry-run",
            Mode::Clean => "clean",
        }
    }
}

struct Args {
    plan: PathBuf,
    mode: Mode,
}

fn parse_args() -> Result<Args> {
    let mut plan = None;
    let mut mode = None;
    let mut arguments = std::env::args().skip(1);

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--plan" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| anyhow!("--plan requires a path"))?;
                plan = Some(PathBuf::from(value));
            }
            "--mode" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| anyhow!("--mode requires a value"))?;
                mode = Some(Mode::parse(&value)?);
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    Ok(Args {
        plan: plan.ok_or_else(|| anyhow!("--plan is required"))?,
        mode: mode.ok_or_else(|| anyhow!("--mode is required"))?,
    })
}

#[derive(Deserialize)]
struct Plan {
    version: u32,
    user_home: String,
    links: Vec<PlannedLink>,
}

#[derive(Deserialize)]
struct PlannedLink {
    link: String,
    target: String,
}

impl Plan {
    fn read(path: &Path) -> Result<Self> {
        let body =
            fs::read_to_string(path).with_context(|| format!("reading plan {}", path.display()))?;
        serde_json::from_str(&body).with_context(|| format!("parsing plan {}", path.display()))
    }
}

fn validate_plan(plan: &Plan) -> Result<()> {
    if plan.version != PLAN_VERSION {
        bail!("unsupported plan version: {}", plan.version);
    }
    if plan.links.is_empty() {
        bail!("the plan contains no links");
    }
    validate_home(&plan.user_home)?;
    validate_unique(&plan.links)?;

    for planned in &plan.links {
        validate_link(&planned.link, &plan.user_home)?;
        validate_target(&planned.link, &planned.target)?;
    }
    Ok(())
}

fn validate_home(home: &str) -> Result<()> {
    let path = Path::new(home);
    if !path.is_absolute() {
        bail!("user_home is not absolute: {home}");
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        bail!("user_home contains ..: {home}");
    }

    let trimmed = home.trim_end_matches('/');
    let forbidden = trimmed.is_empty()
        || trimmed == "/nix"
        || trimmed.starts_with("/nix/")
        || trimmed == "/run"
        || trimmed.starts_with("/run/")
        || trimmed == "/etc"
        || trimmed.starts_with("/etc/");
    if forbidden {
        bail!("refusing unsafe user_home: {home}");
    }
    Ok(())
}

fn validate_unique(links: &[PlannedLink]) -> Result<()> {
    let mut seen = HashSet::new();
    for planned in links {
        if !seen.insert(planned.link.as_str()) {
            bail!("duplicate link in plan: {}", planned.link);
        }
    }
    Ok(())
}

fn validate_link(link: &str, home: &str) -> Result<()> {
    let path = Path::new(link);
    if !path.is_absolute() {
        bail!("link is not absolute: {link}");
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        bail!("link contains ..: {link}");
    }
    if is_denied_location(link) {
        bail!("refusing protected location: {link}");
    }
    if !is_allowed_location(link, home) {
        bail!("link is outside the permitted locations: {link}");
    }
    Ok(())
}

fn is_denied_location(link: &str) -> bool {
    link == "/nix/var/nix/profiles/system" || link.starts_with("/run/") || link.starts_with("/etc/")
}

fn is_allowed_location(link: &str, home: &str) -> bool {
    if ALLOWED_PREFIXES
        .iter()
        .any(|prefix| link.starts_with(prefix))
    {
        return true;
    }
    let home_prefix = format!("{}/", home.trim_end_matches('/'));
    link.starts_with(&home_prefix)
}

fn validate_target(link: &str, target: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(link).with_context(|| format!("reading link {link}"))?;
    if !metadata.file_type().is_symlink() {
        bail!("not a symlink: {link}");
    }
    let resolved = fs::canonicalize(link).with_context(|| format!("resolving link {link}"))?;
    if resolved != Path::new(target) {
        bail!(
            "planned target does not match {link}: planned {target}, current {}",
            resolved.display()
        );
    }
    Ok(())
}

fn verify_discovered(plan: &Plan, messages: &mut Vec<String>) -> Result<()> {
    let discovery = engine::roots::discover(Path::new(&plan.user_home));
    messages.extend(discovery.warnings);

    let roots: HashMap<&str, &engine::model::Root> = discovery
        .roots
        .iter()
        .map(|root| (root.link.as_str(), root))
        .collect();

    for planned in &plan.links {
        let Some(root) = roots.get(planned.link.as_str()) else {
            bail!("unknown link: {}", planned.link);
        };
        if root.protection != Protection::None {
            bail!(
                "refusing {}: protected as {}",
                planned.link,
                root.protection.reason()
            );
        }
        if root.target != planned.target {
            bail!(
                "target changed for {}: planned {}, current {}",
                planned.link,
                planned.target,
                root.target
            );
        }
    }
    Ok(())
}

fn acquire_gc_lock() -> Result<File> {
    let file = File::open(GC_LOCK).with_context(|| format!("opening {GC_LOCK}"))?;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        return Err(error).with_context(|| format!("locking {GC_LOCK}"));
    }
    Ok(file)
}

struct Staged {
    link: String,
    target: String,
    staging: String,
}

fn staging_path(link: &str) -> String {
    format!("{link}{STAGING_SUFFIX}")
}

struct AutoRoot {
    path: PathBuf,
    target: PathBuf,
}

fn snapshot_auto_roots(links: &[PlannedLink]) -> Result<Vec<AutoRoot>> {
    let directory = Path::new(AUTO_ROOTS);
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("reading {}", directory.display()));
        }
    };

    let planned: Vec<&Path> = links
        .iter()
        .map(|planned| Path::new(&planned.link))
        .collect();
    let mut snapshots = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(target) = fs::read_link(&path) else {
            continue;
        };
        let resolved = if target.is_absolute() {
            target.clone()
        } else {
            directory.join(&target)
        };
        if planned.iter().any(|link| resolved.as_path() == *link) {
            snapshots.push(AutoRoot { path, target });
        }
    }
    Ok(snapshots)
}

fn restore_auto_roots(snapshots: &[AutoRoot], messages: &mut Vec<String>) -> Result<()> {
    let mut failures = Vec::new();
    for snapshot in snapshots {
        match fs::symlink_metadata(&snapshot.path) {
            Ok(_) => match fs::read_link(&snapshot.path) {
                Ok(current) if current == snapshot.target => {}
                Ok(current) => failures.push(format!(
                    "automatic GC root {} points to {} instead of {}",
                    snapshot.path.display(),
                    current.display(),
                    snapshot.target.display()
                )),
                Err(error) => failures.push(format!(
                    "reading automatic GC root {}: {error}",
                    snapshot.path.display()
                )),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Err(error) = std::os::unix::fs::symlink(&snapshot.target, &snapshot.path) {
                    failures.push(format!(
                        "recreating automatic GC root {}: {error}",
                        snapshot.path.display()
                    ));
                }
            }
            Err(error) => failures.push(format!(
                "reading automatic GC root {}: {error}",
                snapshot.path.display()
            )),
        }
    }

    if failures.is_empty() {
        if !snapshots.is_empty() {
            messages.push(format!("restored {} automatic GC roots", snapshots.len()));
        }
        Ok(())
    } else {
        bail!(
            "could not restore the automatic GC roots: {}",
            failures.join("; ")
        )
    }
}

fn stage(links: &[PlannedLink], messages: &mut Vec<String>) -> Result<Vec<Staged>> {
    for planned in links {
        let staging = staging_path(&planned.link);
        if fs::symlink_metadata(&staging).is_ok() {
            bail!("staging path already exists: {staging}");
        }
        validate_target(&planned.link, &planned.target)?;
    }

    let mut staged = Vec::new();
    for planned in links {
        let staging = staging_path(&planned.link);
        if let Err(error) = fs::rename(&planned.link, &staging) {
            let message = format!("staging {} failed: {error}", planned.link);
            return match restore(&staged, messages) {
                Ok(()) => Err(anyhow!(message)),
                Err(restore_error) => Err(anyhow!("{message}; {restore_error}")),
            };
        }
        staged.push(Staged {
            link: planned.link.clone(),
            target: planned.target.clone(),
            staging,
        });
    }
    Ok(staged)
}

fn restore(staged: &[Staged], messages: &mut Vec<String>) -> Result<()> {
    let mut failures = Vec::new();

    for entry in staged.iter().rev() {
        if fs::symlink_metadata(&entry.staging).is_ok() {
            if let Err(error) = fs::rename(&entry.staging, &entry.link) {
                failures.push(format!(
                    "moving {} back to {}: {error}",
                    entry.staging, entry.link
                ));
            }
        }
    }

    for entry in staged {
        if let Err(error) = validate_restored(entry) {
            failures.push(error.to_string());
        }
    }

    if failures.is_empty() {
        if !staged.is_empty() {
            messages.push(format!("restored {} roots", staged.len()));
        }
        Ok(())
    } else {
        bail!("could not restore every root: {}", failures.join("; "))
    }
}

fn validate_restored(entry: &Staged) -> Result<()> {
    let metadata = fs::symlink_metadata(&entry.link)
        .with_context(|| format!("root is missing after restore: {}", entry.link))?;
    if !metadata.file_type().is_symlink() {
        bail!("root is not a symlink after restore: {}", entry.link);
    }
    if fs::symlink_metadata(&entry.staging).is_ok() {
        bail!("staging link still exists: {}", entry.staging);
    }
    let resolved = fs::canonicalize(&entry.link)
        .with_context(|| format!("resolving restored root {}", entry.link))?;
    if resolved != Path::new(&entry.target) {
        bail!(
            "restored root points elsewhere: {} -> {}",
            entry.link,
            resolved.display()
        );
    }
    Ok(())
}

fn restore_all(
    staged: &[Staged],
    auto_roots: &[AutoRoot],
    messages: &mut Vec<String>,
) -> Result<()> {
    let links = restore(staged, messages);
    let roots = restore_auto_roots(auto_roots, messages);
    match (links, roots) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(link_error), Ok(())) => Err(link_error),
        (Ok(()), Err(root_error)) => Err(root_error),
        (Err(link_error), Err(root_error)) => Err(anyhow!("{link_error}; {root_error}")),
    }
}

fn dry_run(
    staged: &[Staged],
    auto_roots: &[AutoRoot],
    messages: &mut Vec<String>,
) -> Result<(usize, u64)> {
    let paths = list_dead_paths()?;
    let bytes = dead_bytes(&paths, messages)?;
    restore_all(staged, auto_roots, messages)?;
    Ok((paths.len(), bytes))
}

fn list_dead_paths() -> Result<Vec<String>> {
    let output = nix_command("nix-store")?
        .arg("--gc")
        .arg("--print-dead")
        .output()
        .context("running nix-store --gc --print-dead")?;
    if !output.status.success() {
        bail!("nix-store --gc --print-dead failed: {}", describe(&output));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("/nix/store/"))
        .map(str::to_string)
        .collect())
}

fn dead_bytes(paths: &[String], messages: &mut Vec<String>) -> Result<u64> {
    let mut total = 0u64;
    for batch in paths.chunks(SIZE_BATCH) {
        let output = nix_command("nix-store")?
            .arg("--query")
            .arg("--size")
            .args(batch)
            .output()
            .context("running nix-store --query --size")?;
        if !output.status.success() {
            bail!("nix-store --query --size failed: {}", describe(&output));
        }

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match line.parse::<u64>() {
                Ok(size) => total = total.saturating_add(size),
                Err(_) => messages.push(format!("ignored unexpected size output: {line}")),
            }
        }
    }
    Ok(total)
}

fn clean(staged: &[Staged], messages: &mut Vec<String>) -> Result<()> {
    for entry in staged {
        fs::symlink_metadata(&entry.staging)
            .with_context(|| format!("staged root is missing: {}", entry.staging))?;
        if fs::symlink_metadata(&entry.link).is_ok() {
            bail!("original root still exists: {}", entry.link);
        }
    }

    let output = nix_command("nix-collect-garbage")?
        .output()
        .context("running nix-collect-garbage")?;
    if !output.status.success() {
        bail!("nix-collect-garbage failed: {}", describe(&output));
    }

    let mut leftovers = Vec::new();
    for entry in staged {
        if let Err(error) = fs::remove_file(&entry.staging) {
            leftovers.push(format!("{}: {error}", entry.staging));
        }
    }
    if leftovers.is_empty() {
        messages.push(format!(
            "removed {} roots and cleaned the store",
            staged.len()
        ));
    } else {
        messages.push(format!(
            "the store was cleaned but these staging links remain: {}",
            leftovers.join("; ")
        ));
    }
    Ok(())
}

enum Outcome {
    DryRun { dead_paths: usize, dead_bytes: u64 },
    Clean,
}

fn run(messages: &mut Vec<String>) -> Result<Value> {
    let args = parse_args()?;
    let plan = Plan::read(&args.plan)?;
    validate_plan(&plan)?;

    let _lock = acquire_gc_lock()?;
    verify_discovered(&plan, messages)?;

    let auto_roots = snapshot_auto_roots(&plan.links)?;
    let staged = stage(&plan.links, messages)?;
    let outcome = match args.mode {
        Mode::DryRun => dry_run(&staged, &auto_roots, messages).map(|(dead_paths, dead_bytes)| {
            Outcome::DryRun {
                dead_paths,
                dead_bytes,
            }
        }),
        Mode::Clean => clean(&staged, messages).map(|()| Outcome::Clean),
    };

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            messages.push(format!("aborting: {error}"));
            return match restore_all(&staged, &auto_roots, messages) {
                Ok(()) => Err(error),
                Err(restore_error) => Err(anyhow!(
                    "{error}; restoring the roots failed: {restore_error}"
                )),
            };
        }
    };

    let value = match outcome {
        Outcome::DryRun {
            dead_paths,
            dead_bytes,
        } => json!({
            "ok": true,
            "mode": args.mode.name(),
            "dead_paths": dead_paths,
            "dead_bytes": dead_bytes,
            "cleaned": false,
            "restored": true,
            "messages": messages.clone(),
        }),
        Outcome::Clean => json!({
            "ok": true,
            "mode": args.mode.name(),
            "dead_paths": 0,
            "dead_bytes": 0,
            "cleaned": true,
            "restored": false,
            "messages": messages.clone(),
        }),
    };
    Ok(value)
}

fn nix_command(program: &str) -> Result<Command> {
    let executable = resolve_program(program)?;
    let mut command = Command::new(executable);
    command.env("PATH", CHILD_PATH);
    Ok(command)
}

fn resolve_program(program: &str) -> Result<PathBuf> {
    for directory in CHILD_PATH.split(':') {
        let candidate = Path::new(directory).join(program);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("{program} was not found in {CHILD_PATH}");
}

fn describe(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        format!("exited with {}", output.status)
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/den";

    #[test]
    fn denies_protected_locations() {
        assert!(is_denied_location("/nix/var/nix/profiles/system"));
        assert!(is_denied_location("/run/current-system"));
        assert!(is_denied_location("/etc/nixos"));
        assert!(!is_denied_location("/nix/var/nix/profiles/system-149-link"));
    }

    #[test]
    fn allows_only_permitted_prefixes() {
        assert!(is_allowed_location(
            "/nix/var/nix/profiles/system-149-link",
            HOME
        ));
        assert!(is_allowed_location(
            "/nix/var/nix/gcroots/per-user/den/root",
            HOME
        ));
        assert!(is_allowed_location("/home/den/repos/foo/result", HOME));
        assert!(!is_allowed_location("/nix/var/nix/gcroots/auto/abc", HOME));
        assert!(!is_allowed_location("/home/other/result", HOME));
    }

    #[test]
    fn rejects_unsafe_home() {
        assert!(validate_home("/").is_err());
        assert!(validate_home("/nix").is_err());
        assert!(validate_home("/etc").is_err());
        assert!(validate_home("home/den").is_err());
        assert!(validate_home("/home/den/../root").is_err());
        assert!(validate_home("/home/den").is_ok());
    }

    #[test]
    fn rejects_duplicate_links() {
        let links = vec![
            PlannedLink {
                link: "/nix/var/nix/profiles/system-1-link".to_string(),
                target: "/nix/store/a".to_string(),
            },
            PlannedLink {
                link: "/nix/var/nix/profiles/system-1-link".to_string(),
                target: "/nix/store/a".to_string(),
            },
        ];
        let error = validate_unique(&links).unwrap_err();
        assert!(error.to_string().contains("duplicate"));
    }

    #[test]
    fn staging_name_is_adjacent() {
        assert_eq!(
            staging_path("/nix/var/nix/profiles/system-149-link"),
            "/nix/var/nix/profiles/system-149-link.nix-cleaner-staging"
        );
    }
}
