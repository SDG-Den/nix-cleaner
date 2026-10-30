use std::time::{SystemTime, UNIX_EPOCH};

pub fn bytes(bytes: u64) -> String {
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

pub fn age(timestamp: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let elapsed = (now - timestamp).max(0);
    match elapsed {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", elapsed / 60),
        3600..=86399 => format!("{} h ago", elapsed / 3600),
        _ => format!("{} days ago", elapsed / 86400),
    }
}

fn shorten(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        return label.to_string();
    }
    let mut shortened: String = label.chars().take(max_chars.saturating_sub(1)).collect();
    shortened.push('…');
    shortened
}

pub fn join_limited(labels: &[String], limit: usize) -> String {
    if labels.is_empty() {
        return "unused".to_string();
    }
    let shown: Vec<String> = labels
        .iter()
        .take(limit)
        .map(|label| shorten(label, 22))
        .collect();
    if labels.len() <= limit {
        return shown.join(", ");
    }
    format!("{}, +{} more", shown.join(", "), labels.len() - limit)
}
