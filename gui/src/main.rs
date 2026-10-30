mod app;
mod clean;
mod format;
mod panels;
mod progress;
mod rows;
mod state;
mod ui;

use gtk::prelude::*;

fn main() -> gtk::glib::ExitCode {
    let application = adw::Application::builder()
        .application_id("dev.den.NixCleaner")
        .build();
    application.connect_activate(app::activate);
    application.run()
}
