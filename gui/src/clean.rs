use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::ui;

#[derive(Debug, Clone, Serialize)]
pub struct PlanLink {
    pub link: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
pub struct HelperOutput {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub dead_bytes: u64,
    #[serde(default)]
    pub dead_paths: usize,
    #[serde(default)]
    pub cleaned: bool,
    #[serde(default)]
    pub restored: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub messages: Vec<String>,
}

pub fn write_plan(links: &[PlanLink]) -> std::io::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "nix-cleaner-plan-{}.json",
        std::process::id()
    ));
    let home = engine::cache::home_dir();
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    let body = serde_json::json!({
        "version": 1,
        "user_home": home.to_string_lossy(),
        "created_at": created_at,
        "links": links,
    });

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    file.write_all(serde_json::to_string_pretty(&body)?.as_bytes())?;
    drop(file);
    Ok(path)
}

pub fn spawn(
    links: Vec<PlanLink>,
    mode: &'static str,
    done: impl Fn(Result<HelperOutput, String>) + 'static,
) {
    let Some(helper) = ui::helper_path() else {
        done(Err(
            "nix-cleaner-helper was not found next to the application or on PATH".to_string(),
        ));
        return;
    };

    let plan_path = match write_plan(&links) {
        Ok(path) => path,
        Err(error) => {
            done(Err(format!("could not write the plan file: {error}")));
            return;
        }
    };

    let (sender, receiver) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let (success, output) = ui::run_pkexec(&helper, &plan_path, mode);
        let _ = std::fs::remove_file(&plan_path);
        let result = if success {
            match serde_json::from_str::<HelperOutput>(output.trim()) {
                Ok(parsed) => Ok(parsed),
                Err(error) => Err(format!(
                    "unexpected helper output ({error}): {}",
                    output.trim()
                )),
            }
        } else {
            Err(output)
        };
        let _ = sender.send_blocking(result);
    });

    let done = Rc::new(done);
    gtk::glib::spawn_future_local(async move {
        if let Ok(result) = receiver.recv().await {
            done(result);
        }
    });
}
