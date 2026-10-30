use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use crate::format;
use crate::panels::roots::RootsPanel;
use crate::panels::store::StorePanel;
use crate::panels::tree::TreePanel;
use crate::progress::ProgressView;
use crate::state::{SharedState, State};
use crate::ui;

pub struct App {
    pub window: adw::ApplicationWindow,
    state: SharedState,
    roots: Rc<RootsPanel>,
    store: Rc<StorePanel>,
    tree: Rc<TreePanel>,
    progress: Rc<ProgressView>,
    title: adw::WindowTitle,
    scan_button: gtk::Button,
    holder: Rc<RefCell<Option<std::rc::Weak<App>>>>,
}

thread_local! {
    static KEEPALIVE: RefCell<Vec<Rc<App>>> = const { RefCell::new(Vec::new()) };
}

pub fn activate(application: &adw::Application) {
    let app = Rc::new(App::new(application));
    *app.holder.borrow_mut() = Some(Rc::downgrade(&app));
    app.window.present();

    let loading = app.clone();
    glib::idle_add_local_once(move || {
        loading.load_cache();
    });

    KEEPALIVE.with(|keepalive| keepalive.borrow_mut().push(app));
}

impl App {
    fn new(application: &adw::Application) -> App {
        let state = State::new();
        let window = adw::ApplicationWindow::builder()
            .application(application)
            .title("Nix Cleaner")
            .default_width(1180)
            .default_height(760)
            .build();

        let gtk_window = window.clone().upcast::<gtk::Window>();
        let roots = RootsPanel::new(&gtk_window, state.clone());
        let store = StorePanel::new(state.clone());
        let tree = TreePanel::new(state.clone());
        let progress = ProgressView::new();

        let holder: Rc<RefCell<Option<std::rc::Weak<App>>>> = Rc::new(RefCell::new(None));
        let trigger: Rc<dyn Fn()> = {
            let holder = holder.clone();
            Rc::new(move || {
                if let Some(app) = holder.borrow().as_ref().and_then(|weak| weak.upgrade()) {
                    app.start_scan();
                }
            })
        };
        roots.set_rescan(trigger.clone());

        let header = adw::HeaderBar::new();
        let title = adw::WindowTitle::new("Nix Cleaner", "no index yet");
        header.set_title_widget(Some(&title));

        let scan_button = gtk::Button::with_label("Scan");
        scan_button.add_css_class("suggested-action");
        let validate_button = gtk::Button::with_label("Validate");
        header.pack_end(&scan_button);
        header.pack_end(&validate_button);

        let stack = adw::ViewStack::new();
        stack.set_vexpand(true);
        stack.set_hexpand(true);
        stack.add_titled(&roots.widget, Some("roots"), "GC Roots");
        stack.add_titled(&store.widget, Some("store"), "Store");
        stack.add_titled(&tree.widget, Some("tree"), "Dependency Tree");

        let switcher = adw::ViewSwitcher::builder()
            .policy(adw::ViewSwitcherPolicy::Wide)
            .build();
        switcher.set_stack(Some(&stack));
        let switcher_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        switcher_bar.set_halign(gtk::Align::Center);
        switcher_bar.set_margin_top(6);
        switcher_bar.set_margin_bottom(6);
        switcher_bar.append(&switcher);

        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        footer.set_margin_start(12);
        footer.set_margin_end(12);
        footer.set_margin_top(6);
        footer.set_margin_bottom(6);
        footer.append(&progress.label);
        footer.append(&progress.bar);
        footer.append(&progress.button);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&switcher_bar);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&stack);
        content.append(&progress.panel);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&footer);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&content));
        window.set_content(Some(&toolbar));

        let app = App {
            window,
            state,
            roots,
            store,
            tree,
            progress,
            title,
            scan_button,
            holder,
        };

        let weak = app.holder.clone();
        app.scan_button.connect_clicked(move |_| {
            if let Some(app) = weak.borrow().as_ref().and_then(|weak| weak.upgrade()) {
                app.start_scan();
            }
        });

        let weak = app.holder.clone();
        validate_button.connect_clicked(move |_| {
            if let Some(app) = weak.borrow().as_ref().and_then(|weak| weak.upgrade()) {
                app.validate();
            }
        });

        app
    }

    fn load_cache(self: &Rc<Self>) {
        match engine::cache::load() {
            Ok(Some(index)) => {
                let scanned_at = index.scanned_at;
                self.state.set_loaded(index);
                self.refresh();
                self.title.set_subtitle(&format!(
                    "index from {}, press Scan to refresh",
                    format::age(scanned_at)
                ));
                self.progress
                    .log(&format!("loaded cached index from {}", format::age(scanned_at)));
            }
            Ok(None) => {
                self.progress
                    .log("no cached index yet, press Scan or Enter to start");
                self.title.set_subtitle("no index yet");
            }
            Err(error) => {
                self.progress.fail(&format!("could not read the cache: {error}"));
            }
        }
    }

    fn start_scan(self: &Rc<Self>) {
        if self.state.scanning.get() {
            return;
        }
        self.state.scanning.set(true);
        self.scan_button.set_sensitive(false);
        self.progress.reset("starting scan");
        self.state.log.borrow_mut().clear();

        let (sender, receiver) = async_channel::unbounded();
        std::thread::spawn(move || {
            engine::scan::scan_and_report(&mut |event| {
                let _ = sender.send_blocking(event);
            });
        });

        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(event) = receiver.recv().await {
                let Some(app) = weak.upgrade() else {
                    break;
                };
                app.handle_event(event);
            }
        });
    }

    fn handle_event(self: &Rc<Self>, event: engine::progress::ScanEvent) {
        match event {
            engine::progress::ScanEvent::Stage { name } => {
                self.progress.set_stage(&name);
            }
            engine::progress::ScanEvent::Progress {
                stage,
                done,
                total,
                message,
            } => {
                self.progress.set_progress(&stage, done, total, &message);
            }
            engine::progress::ScanEvent::Log(line) => {
                self.progress.log(&line);
            }
            engine::progress::ScanEvent::Done(index) => {
                let scanned_at = index.scanned_at;
                let path_count = index.paths.len();
                let root_count = index.roots.len();
                self.state.set_loaded(*index);
                self.state.scanning.set(false);
                self.scan_button.set_sensitive(true);
                self.refresh();
                self.title.set_subtitle(&format!(
                    "{} paths · {} roots · scanned {}",
                    path_count,
                    root_count,
                    format::age(scanned_at)
                ));
                self.progress.finish(&format!(
                    "scan complete · {path_count} paths · {root_count} gc roots"
                ));
            }
            engine::progress::ScanEvent::Failed(error) => {
                self.state.scanning.set(false);
                self.scan_button.set_sensitive(true);
                self.progress.fail(&format!("scan failed: {error}"));
            }
        }
    }

    fn refresh(self: &Rc<Self>) {
        self.roots.rebuild();
        self.store.set_data();
        self.tree.set_data();
    }

    fn validate(self: &Rc<Self>) {
        let Some(loaded) = self.state.loaded.borrow().clone() else {
            ui::info(&self.window, "Validate", "Run a scan first");
            return;
        };

        let (sender, receiver) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let result = engine::validate::run(&loaded.index, &loaded.graph);
            let _ = sender.send_blocking(result);
        });

        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(result) = receiver.recv().await else {
                return;
            };
            let Some(app) = weak.upgrade() else {
                return;
            };
            let body = format!(
                "store paths: {} ({} live, {} dead)\n\
                 our live bytes: {}\n\
                 our dead bytes: {}\n\n\
                 nix reports {} dead paths, {}\n\
                 df reports {} used on the store filesystem\n\n\
                 difference between our dead set and nix: {}",
                result.store_paths,
                result.live_paths,
                result.dead_paths,
                format::bytes(result.live_bytes),
                format::bytes(result.dead_bytes),
                result.nix_dead_paths,
                format::bytes(result.nix_dead_bytes),
                format::bytes(result.df_used),
                format::bytes(result.dead_bytes.abs_diff(result.nix_dead_bytes)),
            );
            ui::info(&app.window, "Validation", &body);
        });
    }
}
