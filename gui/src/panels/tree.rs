use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use engine::attribution::{build_tree, TreeNode};
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::format;
use crate::rows::{self, Cell, Column, ColumnKind, Marker, Row};
use crate::state::SharedState;

static COLUMNS: [Column; 3] = [
    Column {
        title: "Path",
        width: 0,
        align: gtk::Align::Start,
        kind: ColumnKind::Text,
    },
    Column {
        title: "With dependencies",
        width: 160,
        align: gtk::Align::End,
        kind: ColumnKind::Text,
    },
    Column {
        title: "Standalone size",
        width: 150,
        align: gtk::Align::End,
        kind: ColumnKind::Text,
    },
];

struct ArenaNode {
    path: String,
    own: u64,
    cumulative: u64,
    shared: bool,
    children: Vec<usize>,
}

struct Tree {
    nodes: Vec<ArenaNode>,
    expanded: HashSet<usize>,
}

fn build_arena(node: &TreeNode, arena: &mut Vec<ArenaNode>) -> usize {
    let index = arena.len();
    arena.push(ArenaNode {
        path: node.path.clone(),
        own: node.own,
        cumulative: node.cumulative,
        shared: node.shared,
        children: Vec::new(),
    });
    let children: Vec<usize> = node
        .children
        .iter()
        .map(|child| build_arena(child, arena))
        .collect();
    arena[index].children = children;
    index
}

pub struct TreePanel {
    pub widget: gtk::Box,
    picker: gtk::DropDown,
    list: gtk::ListView,
    store: gio::ListStore,
    summary: gtk::Label,
    roots: RefCell<Vec<u32>>,
    tree: RefCell<Option<Tree>>,
    state: SharedState,
}

impl TreePanel {
    pub fn new(state: SharedState) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_hexpand(true);
        widget.set_vexpand(true);

        let picker = gtk::DropDown::from_strings(&["Scan the store first"]);
        picker.set_hexpand(true);

        let measure = gtk::Button::with_label("Measure on disk");
        measure.set_valign(gtk::Align::Center);

        let picker_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        picker_bar.set_margin_top(8);
        picker_bar.set_margin_bottom(8);
        picker_bar.set_margin_start(12);
        picker_bar.set_margin_end(12);
        picker_bar.append(&picker);
        picker_bar.append(&measure);

        let expand_all = gtk::Button::with_label("Expand all");
        expand_all.set_valign(gtk::Align::Center);
        let collapse_all = gtk::Button::with_label("Collapse");
        collapse_all.set_valign(gtk::Align::Center);
        picker_bar.append(&expand_all);
        picker_bar.append(&collapse_all);

        let summary = gtk::Label::new(None);
        summary.set_xalign(0.0);
        summary.add_css_class("dim-label");
        summary.set_margin_start(12);
        summary.set_margin_end(12);
        summary.set_margin_bottom(6);

        let (list, store) = rows::list_view(&COLUMNS);

        widget.append(&picker_bar);
        widget.append(&rows::header_row(&COLUMNS));
        widget.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        widget.append(&rows::scrolled(&list));
        widget.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        widget.append(&summary);

        let panel = Rc::new(TreePanel {
            widget,
            picker,
            list,
            store,
            summary,
            roots: RefCell::new(Vec::new()),
            tree: RefCell::new(None),
            state,
        });

        panel.list.set_single_click_activate(true);
        let weak = Rc::downgrade(&panel);
        panel.list.connect_activate(move |list, position| {
            let Some(panel) = weak.upgrade() else {
                return;
            };
            let Some(model) = list.model() else {
                return;
            };
            let Some(object) = model.item(position).and_downcast::<glib::BoxedAnyObject>() else {
                return;
            };
            let row = object.borrow::<Row>().clone();
            {
                let mut guard = panel.tree.borrow_mut();
                let Some(tree) = guard.as_mut() else {
                    return;
                };
                let index = row.id as usize;
                let Some(node) = tree.nodes.get(index) else {
                    return;
                };
                if node.children.is_empty() {
                    return;
                }
                if !tree.expanded.remove(&index) {
                    tree.expanded.insert(index);
                }
            }
            panel.render();
        });

        let weak = Rc::downgrade(&panel);
        measure.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.measure_root();
            }
        });

        let weak = Rc::downgrade(&panel);
        expand_all.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.set_all_expanded(true);
            }
        });

        let weak = Rc::downgrade(&panel);
        collapse_all.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.set_all_expanded(false);
            }
        });

        let weak = Rc::downgrade(&panel);
        panel.picker.connect_selected_notify(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.load();
            }
        });

        panel
    }

    pub fn set_data(self: &Rc<Self>) {
        let loaded = self.state.loaded.borrow().clone();
        let Some(loaded) = loaded else {
            self.roots.borrow_mut().clear();
            *self.tree.borrow_mut() = None;
            rows::replace_rows(&self.store, Vec::new());
            self.summary.set_label("");
            return;
        };

        let mut order: Vec<u32> = (0..loaded.index.roots.len() as u32)
            .filter(|index| loaded.index.analysis.closure_sizes[*index as usize] > 0)
            .collect();
        order.sort_by(|left, right| {
            loaded.index.analysis.closure_sizes[*right as usize]
                .cmp(&loaded.index.analysis.closure_sizes[*left as usize])
        });

        let labels: Vec<String> = order
            .iter()
            .map(|index| {
                format!(
                    "{} · {}",
                    loaded.index.roots[*index as usize].label,
                    format::bytes(loaded.index.analysis.closure_sizes[*index as usize])
                )
            })
            .collect();

        self.picker.set_model(Some(&gtk::StringList::new(
            &labels.iter().map(String::as_str).collect::<Vec<_>>(),
        )));
        self.picker.set_selected(0);

        *self.roots.borrow_mut() = order;
        self.load();
    }

    fn load(self: &Rc<Self>) {
        let loaded = match self.state.loaded.borrow().clone() {
            Some(loaded) => loaded,
            None => return,
        };
        let roots = self.roots.borrow().clone();
        let Some(root_index) = roots.get(self.picker.selected() as usize).copied() else {
            *self.tree.borrow_mut() = None;
            rows::replace_rows(&self.store, Vec::new());
            return;
        };
        let Some(root) = loaded.index.roots.get(root_index as usize) else {
            return;
        };
        let Some(tree) = build_tree(&loaded.graph, root, &loaded.index.policy) else {
            *self.tree.borrow_mut() = None;
            rows::replace_rows(&self.store, Vec::new());
            self.summary
                .set_label("this root no longer resolves inside the store");
            return;
        };

        let mut arena = Vec::new();
        build_arena(&tree, &mut arena);
        let mut expanded = HashSet::new();
        if !arena.is_empty() {
            expanded.insert(0);
        }
        *self.tree.borrow_mut() = Some(Tree {
            nodes: arena,
            expanded,
        });
        self.render();
    }

    fn set_all_expanded(self: &Rc<Self>, expand: bool) {
        if let Some(tree) = self.tree.borrow_mut().as_mut() {
            if expand {
                tree.expanded = (0..tree.nodes.len()).collect();
            } else {
                tree.expanded.clear();
                tree.expanded.insert(0);
            }
        }
        self.render();
    }

    fn render(self: &Rc<Self>) {
        let tree = self.tree.borrow();
        let Some(tree) = tree.as_ref() else {
            rows::replace_rows(&self.store, Vec::new());
            self.summary.set_label("");
            return;
        };
        if tree.nodes.is_empty() {
            rows::replace_rows(&self.store, Vec::new());
            return;
        }

        let mut rows = Vec::new();
        render_node(tree, 0, 0, &mut rows);
        rows::replace_rows(&self.store, rows);

        let root = &tree.nodes[0];
        self.summary.set_label(&format!(
            "{} · {} paths · {} including dependencies",
            engine::names::pretty_name(&root.path),
            tree.nodes.len(),
            format::bytes(root.cumulative)
        ));
    }

    fn measure_root(self: &Rc<Self>) {
        let Some(loaded) = self.state.loaded.borrow().clone() else {
            return;
        };
        let roots = self.roots.borrow().clone();
        let Some(root_index) = roots.get(self.picker.selected() as usize).copied() else {
            return;
        };
        let Some(root) = loaded.index.roots.get(root_index as usize).cloned() else {
            return;
        };
        let Some(start) = loaded.graph.get(&root.target) else {
            return;
        };

        let policy = loaded.index.policy;
        let label = root.label.clone();
        let summary = self.summary.clone();
        summary.set_label(&format!("measuring on-disk usage of {label} …"));

        let (sender, receiver) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let usage =
                engine::disk::closure_usage(&loaded.graph, start, &policy, &mut |_, _, _| {});
            let _ = sender.send_blocking(usage);
        });

        glib::spawn_future_local(async move {
            if let Ok(usage) = receiver.recv().await {
                summary.set_label(&format!(
                    "{label} · on disk {} · {} shared with other store paths · {} files",
                    format::bytes(usage.bytes),
                    format::bytes(usage.shared_bytes),
                    usage.files
                ));
            }
        });
    }
}

fn render_node(tree: &Tree, index: usize, depth: u32, rows: &mut Vec<Row>) {
    let node = &tree.nodes[index];
    let has_children = !node.children.is_empty();
    let expanded = tree.expanded.contains(&index);

    rows.push(Row {
        id: index as u32,
        depth,
        cells: vec![
            Cell::Text(engine::names::pretty_name(&node.path)),
            Cell::Text(if node.shared {
                "shared".to_string()
            } else {
                format::bytes(node.cumulative)
            }),
            Cell::Text(format::bytes(node.own)),
        ],
        marker: if !has_children {
            Marker::None
        } else if expanded {
            Marker::Expanded
        } else {
            Marker::Collapsed
        },
    });

    if expanded {
        for &child in &node.children {
            render_node(tree, child, depth + 1, rows);
        }
    }
}
