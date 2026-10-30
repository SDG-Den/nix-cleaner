use serde::{Deserialize, Serialize};

use crate::gc::GcPolicy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathInfo {
    pub path: String,
    pub nar_size: u64,
    pub references: Vec<String>,
    pub deriver: Option<String>,
    pub registration_time: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RootKind {
    SystemCurrent,
    SystemGeneration,
    BootedSystem,
    HomeManagerCurrent,
    HomeManagerGeneration,
    Profile,
    ResultLink,
    Gcroot,
    Other,
}

impl RootKind {
    pub fn label(self) -> &'static str {
        match self {
            RootKind::SystemCurrent => "current system",
            RootKind::SystemGeneration => "system generation",
            RootKind::BootedSystem => "booted system",
            RootKind::HomeManagerCurrent => "current home manager",
            RootKind::HomeManagerGeneration => "home manager generation",
            RootKind::Profile => "profile",
            RootKind::ResultLink => "result link",
            RootKind::Gcroot => "gc root",
            RootKind::Other => "other",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            RootKind::SystemCurrent => 0,
            RootKind::BootedSystem => 1,
            RootKind::SystemGeneration => 2,
            RootKind::HomeManagerCurrent => 3,
            RootKind::HomeManagerGeneration => 4,
            RootKind::Profile => 5,
            RootKind::ResultLink => 6,
            RootKind::Gcroot => 7,
            RootKind::Other => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protection {
    None,
    CurrentSystem,
    BootedSystem,
    CurrentGeneration,
    NewestGeneration,
    CurrentProfile,
    SystemOwned,
}

impl Protection {
    pub fn is_protected(self) -> bool {
        self != Protection::None
    }

    pub fn reason(self) -> &'static str {
        match self {
            Protection::None => "",
            Protection::CurrentSystem => "the running system",
            Protection::BootedSystem => "the booted system",
            Protection::CurrentGeneration => "the active generation",
            Protection::NewestGeneration => "one of the newest three generations",
            Protection::CurrentProfile => "an active profile",
            Protection::SystemOwned => "a system managed root",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub link: String,
    pub target: String,
    pub kind: RootKind,
    pub label: String,
    pub generation: Option<u32>,
    pub protection: Protection,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Analysis {
    pub closure_sizes: Vec<u64>,
    pub individual_sizes: Vec<u64>,
    pub path_roots: Vec<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub cache_version: u32,
    pub scanned_at: i64,
    pub paths: Vec<PathInfo>,
    pub roots: Vec<Root>,
    pub analysis: Analysis,
    pub live_bytes: u64,
    pub policy: GcPolicy,
    pub warnings: Vec<String>,
}
