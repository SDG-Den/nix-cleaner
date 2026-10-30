use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::format;
use crate::rows::{self, Cell, Column, ColumnKind, Marker, Row};
use crate::state::SharedState;

static COLUMNS: [Column; 4] = [
    Column {
        title: "Item",
        width: 0,
        align: gtk::Align::Start,
        kind: ColumnKind::Text,
    },
    Column {
        title: "Duplicates",
        width: 110,
        align: gtk::Align::End,
        kind: ColumnKind::Text,
    },
    Column {
        title: "Item size",
        width: 120,
        align: gtk::Align::End,
        kind: ColumnKind::Text,
    },
    Column {
        title: "GC roots",
        width: 680,
        align: gtk::Align::Start,
        kind: ColumnKind::Text,
    },
];

struct Group {
    base: String,
    items: Vec<u32>,
    total: u64,
    roots: Vec<u32>,
}

pub struct StorePanel {
    pub widget: gtk::Box,
    list: gtk::ListView,
    store: gio::ListStore,
    search: gtk::SearchEntry,
    sort: gtk::DropDown,
    groups: RefCell<Vec<Group>>,
    expanded: RefCell<HashSet<String>>,
    state: SharedState,
}

impl StorePanel {
    pub fn new(state: SharedState) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_hexpand(true);
        widget.set_vexpand(true);

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("filter by package name or path"));
        search.set_hexpand(true);
        search.set_margin_top(8);
        search.set_margin_bottom(8);
        search.set_margin_start(12);
        search.set_margin_end(12);

        let sort = gtk::DropDown::from_strings(&["Largest", "Name", "Most duplicates"]);
        sort.set_margin_end(12);
        sort.set_valign(gtk::Align::Center);

        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        toolbar.append(&search);
        toolbar.append(&sort);

        let (list, store) = rows::list_view(&COLUMNS);

        widget.append(&toolbar);
        widget.append(&rows::header_row(&COLUMNS));
        widget.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        widget.append(&rows::scrolled(&list));

        let panel = Rc::new(StorePanel {
            widget,
            list,
            store,
            search,
            sort,
            groups: RefCell::new(Vec::new()),
            expanded: RefCell::new(HashSet::new()),
            state,
        });

        let weak = Rc::downgrade(&panel);
        panel.search.connect_search_changed(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.rebuild();
            }
        });

        let weak = Rc::downgrade(&panel);
        panel.sort.connect_selected_notify(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.rebuild();
            }
        });

        let weak = Rc::downgrade(&panel);
        panel.list.set_single_click_activate(true);
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
            if row.marker == Marker::None {
                return;
            }
            let base = match row.cells.first() {
                Some(Cell::Text(text)) => text.clone(),
                _ => return,
            };
            {
                let mut expanded = panel.expanded.borrow_mut();
                if !expanded.remove(&base) {
                    expanded.insert(base);
                }
            }
            panel.rebuild();
        });

        panel
    }

    pub fn set_data(self: &Rc<Self>) {
        let loaded = self.state.loaded.borrow().clone();
        let Some(loaded) = loaded else {
            self.groups.borrow_mut().clear();
            self.rebuild();
            return;
        };

        let mut groups: BTreeMap<String, Group> = BTreeMap::new();
        for (path_index, info) in loaded.index.paths.iter().enumerate() {
            let name = engine::names::store_basename(&info.path);
            let base = engine::names::package_base(name).to_string();
            let group = groups.entry(base.clone()).or_insert_with(|| Group {
                base,
                items: Vec::new(),
                total: 0,
                roots: Vec::new(),
            });
            group.items.push(path_index as u32);
            group.total += info.nar_size;
            for &owner in &loaded.index.analysis.path_roots[path_index] {
                if !group.roots.contains(&owner) {
                    group.roots.push(owner);
                }
            }
        }

        let mut groups: Vec<Group> = groups.into_values().collect();
        for group in &mut groups {
            group.items.sort_by(|left, right| {
                let left_size = loaded.index.paths[*left as usize].nar_size;
                let right_size = loaded.index.paths[*right as usize].nar_size;
                right_size.cmp(&left_size)
            });
        }
        *self.groups.borrow_mut() = groups;
        self.rebuild();
    }

    pub fn rebuild(self: &Rc<Self>) {
        let loaded = self.state.loaded.borrow().clone();
        let Some(loaded) = loaded else {
            rows::replace_rows(&self.store, Vec::new());
            return;
        };

        let query = self.search.text().to_string().to_lowercase();
        let groups_ref = self.groups.borrow();
        let mut groups: Vec<&Group> = groups_ref
            .iter()
            .filter(|group| {
                if query.is_empty() {
                    return true;
                }
                if group.base.to_lowercase().contains(&query) {
                    return true;
                }
                group
                    .items
                    .iter()
                    .any(|item| loaded.index.paths[*item as usize].path.contains(&query))
            })
            .collect();

        match self.sort.selected() {
            1 => groups.sort_by(|left, right| left.base.cmp(&right.base)),
            2 => groups.sort_by(|left, right| {
                right
                    .items
                    .len()
                    .cmp(&left.items.len())
                    .then_with(|| right.total.cmp(&left.total))
            }),
            _ => groups.sort_by(|left, right| right.total.cmp(&left.total)),
        }

        let expanded = self.expanded.borrow();
        let mut rows = Vec::new();

        for group in groups {
            let is_expanded = expanded.contains(&group.base);
            let root_labels: Vec<String> = group
                .roots
                .iter()
                .map(|root| loaded.index.roots[*root as usize].label.clone())
                .collect();

            rows.push(Row {
                id: 0,
                depth: 0,
                cells: vec![
                    Cell::Text(group.base.clone()),
                    Cell::Text(format!("{} paths", group.items.len())),
                    Cell::Text(format::bytes(group.total)),
                    Cell::Text(format::join_limited(&root_labels, 2)),
                ],
                marker: if is_expanded {
                    Marker::Expanded
                } else {
                    Marker::Collapsed
                },
            });

            if is_expanded {
                for item in &group.items {
                    let info = &loaded.index.paths[*item as usize];
                    let name = engine::names::store_basename(&info.path);
                    let version = engine::names::version_of(name).unwrap_or("");
                    let item_roots: Vec<String> = loaded.index.analysis.path_roots[*item as usize]
                        .iter()
                        .map(|root| loaded.index.roots[*root as usize].label.clone())
                        .collect();
                    rows.push(Row {
                        id: 0,
                        depth: 1,
                        cells: vec![
                            Cell::Text(name.to_string()),
                            Cell::Text(version.to_string()),
                            Cell::Text(format::bytes(info.nar_size)),
                            Cell::Text(format::join_limited(&item_roots, 2)),
                        ],
                        marker: Marker::None,
                    });
                }
            }
        }

        rows::replace_rows(&self.store, rows);
    }
}
