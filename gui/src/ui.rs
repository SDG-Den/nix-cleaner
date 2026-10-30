use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

pub fn info(parent: &impl IsA<gtk::Window>, heading: &str, body: &str) {
    let dialog = gtk::Window::builder()
        .title(heading)
        .modal(true)
        .transient_for(parent)
        .default_width(520)
        .build();

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 12);
    layout.set_margin_top(18);
    layout.set_margin_bottom(18);
    layout.set_margin_start(18);
    layout.set_margin_end(18);

    let title = gtk::Label::new(Some(heading));
    title.set_xalign(0.0);
    title.add_css_class("title-4");

    let text = gtk::Label::new(Some(body));
    text.set_xalign(0.0);
    text.set_wrap(true);
    text.set_selectable(true);

    let button = gtk::Button::with_label("Close");
    button.set_halign(gtk::Align::End);

    layout.append(&title);
    layout.append(&text);
    layout.append(&button);
    dialog.set_child(Some(&layout));

    let weak = dialog.downgrade();
    button.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
    });
    dialog.present();
}

pub fn confirm(
    parent: &impl IsA<gtk::Window>,
    heading: &str,
    body: &str,
    accept_label: &str,
    on_accept: impl FnOnce() + 'static,
) {
    let dialog = gtk::Window::builder()
        .title(heading)
        .modal(true)
        .transient_for(parent)
        .default_width(620)
        .build();

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 12);
    layout.set_margin_top(18);
    layout.set_margin_bottom(18);
    layout.set_margin_start(18);
    layout.set_margin_end(18);

    let title = gtk::Label::new(Some(heading));
    title.set_xalign(0.0);
    title.add_css_class("title-4");

    let text = gtk::Label::new(Some(body));
    text.set_xalign(0.0);
    text.set_wrap(true);
    text.set_selectable(true);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);

    let cancel = gtk::Button::with_label("Cancel");
    let accept = gtk::Button::with_label(accept_label);
    accept.add_css_class("destructive-action");
    buttons.append(&cancel);
    buttons.append(&accept);

    layout.append(&title);
    layout.append(&text);
    layout.append(&buttons);
    dialog.set_child(Some(&layout));

    let weak = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
    });

    let callback: Rc<RefCell<Option<Box<dyn FnOnce()>>>> =
        Rc::new(RefCell::new(Some(Box::new(on_accept))));
    let weak = dialog.downgrade();
    accept.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
        let taken = callback.borrow_mut().take();
        if let Some(callback) = taken {
            callback();
        }
    });

    dialog.present();
}

pub fn helper_path() -> Option<std::path::PathBuf> {
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            let candidate = directory.join("nix-cleaner-helper");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    let paths = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&paths) {
        let candidate = directory.join("nix-cleaner-helper");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub fn run_pkexec(helper: &std::path::Path, plan: &std::path::Path, mode: &str) -> (bool, String) {
    let output = std::process::Command::new("pkexec")
        .arg(helper)
        .arg("--plan")
        .arg(plan)
        .arg("--mode")
        .arg(mode)
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if output.status.success() {
                (true, stdout)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                (
                    false,
                    format!(
                        "{}\n{}",
                        stdout.trim(),
                        if stderr.trim().is_empty() {
                            format!("pkexec exited with {}", output.status)
                        } else {
                            stderr.trim().to_string()
                        }
                    ),
                )
            }
        }
        Err(error) => (false, format!("could not run pkexec: {error}")),
    }
}
