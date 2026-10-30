use std::rc::Rc;

use gtk::prelude::*;

pub struct ProgressView {
    pub label: gtk::Label,
    pub bar: gtk::ProgressBar,
    pub button: gtk::Button,
    pub panel: gtk::Revealer,
    buffer: gtk::TextBuffer,
}

impl ProgressView {
    pub fn new() -> Rc<Self> {
        let label = gtk::Label::new(Some("No scan running"));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);

        let bar = gtk::ProgressBar::new();
        bar.set_width_request(180);
        bar.set_show_text(false);
        bar.set_valign(gtk::Align::Center);

        let button = gtk::Button::with_label("Show progress");

        let text_view = gtk::TextView::new();
        text_view.set_editable(false);
        text_view.set_monospace(true);
        text_view.set_wrap_mode(gtk::WrapMode::WordChar);
        text_view.set_top_margin(8);
        text_view.set_bottom_margin(8);
        text_view.set_left_margin(8);
        text_view.set_right_margin(8);
        let buffer = text_view.buffer();

        let scroller = gtk::ScrolledWindow::builder()
            .child(&text_view)
            .height_request(180)
            .vexpand(false)
            .build();

        let title = gtk::Label::new(Some("Scan log"));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("dim-label");

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.set_margin_start(12);
        header.set_margin_end(12);
        header.set_margin_top(6);
        header.set_margin_bottom(6);
        header.append(&title);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&header);
        content.append(&scroller);

        let panel = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .transition_duration(160)
            .reveal_child(false)
            .child(&content)
            .build();

        let view = Rc::new(ProgressView {
            label,
            bar,
            button,
            panel,
            buffer,
        });

        let weak = Rc::downgrade(&view);
        view.button.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                let revealed = view.panel.reveals_child();
                view.panel.set_reveal_child(!revealed);
                view.button.set_label(if revealed {
                    "Show progress"
                } else {
                    "Hide progress"
                });
            }
        });

        view
    }

    pub fn reset(&self, text: &str) {
        self.buffer.set_text("");
        self.label.set_label(text);
        self.bar.set_fraction(0.0);
    }

    pub fn set_stage(&self, stage: &str) {
        self.label.set_label(&format!("scanning: {stage}"));
        self.bar.pulse();
        self.append(&format!("stage: {stage}"));
    }

    pub fn set_progress(&self, stage: &str, done: u32, total: u32, message: &str) {
        if total > 0 {
            self.bar.set_fraction(done as f64 / total as f64);
        } else {
            self.bar.pulse();
        }
        self.label
            .set_label(&format!("{stage} {done}/{total} · {message}"));
    }

    pub fn log(&self, line: &str) {
        self.label.set_label(line);
        self.append(line);
    }

    pub fn append(&self, line: &str) {
        let mut end = self.buffer.end_iter();
        self.buffer.insert(&mut end, line);
        self.buffer.insert(&mut end, "\n");
    }

    pub fn finish(&self, text: &str) {
        self.label.set_label(text);
        self.bar.set_fraction(1.0);
        self.append(text);
    }

    pub fn fail(&self, text: &str) {
        self.label.set_label(text);
        self.bar.set_fraction(0.0);
        self.append(text);
    }
}
