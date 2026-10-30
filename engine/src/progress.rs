use crate::model::Index;

#[derive(Debug, Clone)]
pub enum ScanEvent {
    Stage { name: String },
    Progress {
        stage: String,
        done: u32,
        total: u32,
        message: String,
    },
    Log(String),
    Done(Box<Index>),
    Failed(String),
}

pub type Reporter<'a> = &'a mut dyn FnMut(ScanEvent);

pub fn stage(report: Reporter, name: &str) {
    report(ScanEvent::Stage {
        name: name.to_string(),
    });
}

pub fn progress(report: Reporter, stage: &str, done: u32, total: u32, message: String) {
    report(ScanEvent::Progress {
        stage: stage.to_string(),
        done,
        total,
        message,
    });
}

pub fn log(report: Reporter, message: impl Into<String>) {
    report(ScanEvent::Log(message.into()));
}
