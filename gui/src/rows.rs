use gtk::prelude::*;
use gtk::{gio, glib};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    None,
    Collapsed,
    Expanded,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Icon,
    Checkbox,
}

#[derive(Clone, Copy)]
pub struct Column {
    pub title: &'static str,
    pub width: i32,
    pub align: gtk::Align,
    pub kind: ColumnKind,
}

#[derive(Clone)]
pub enum Cell {
    Text(String),
    Icon {
        name: &'static str,
        fallback: &'static str,
        tooltip: String,
    },
    Checkbox(bool),
}

#[derive(Clone)]
pub struct Row {
    pub id: u32,
    pub depth: u32,
    pub cells: Vec<Cell>,
    pub marker: Marker,
}

fn text_alignment(column: &Column) -> f32 {
    match column.align {
        gtk::Align::End => 1.0,
        gtk::Align::Center => 0.5,
        _ => 0.0,
    }
}

fn make_text_cell(column: &Column) -> gtk::Widget {
    let label = gtk::Label::new(None);
    label.set_xalign(text_alignment(column));
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_halign(gtk::Align::Fill);
    label.set_valign(gtk::Align::Center);

    if column.width <= 0 {
        label.set_hexpand(true);
    } else {
        label.set_width_request(column.width);
        label.set_hexpand(false);
        label.set_max_width_chars(budget(column.width));
    }
    label.upcast()
}

fn budget(width: i32) -> i32 {
    (width / 9).max(4)
}

fn truncate(text: &str, max_chars: i32) -> String {
    let max_chars = max_chars.max(2) as usize;
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut shortened: String = text.chars().take(max_chars - 1).collect();
    shortened.push('…');
    shortened
}

fn make_icon_cell(column: &Column) -> gtk::Widget {
    let stack = gtk::Stack::new();
    stack.set_halign(gtk::Align::Center);
    stack.set_valign(gtk::Align::Center);
    stack.set_width_request(column.width);
    stack.set_hexpand(false);
    stack.add_named(&gtk::Image::new(), Some("icon"));
    let fallback = gtk::Label::new(None);
    fallback.add_css_class("dim-label");
    stack.add_named(&fallback, Some("text"));
    stack.upcast()
}

fn make_checkbox_cell(column: &Column) -> gtk::Widget {
    let label = gtk::Label::new(None);
    label.set_xalign(0.5);
    label.set_width_request(column.width);
    label.set_hexpand(false);
    label.upcast()
}

fn set_text(widget: &gtk::Widget, text: &str, column: &Column) {
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        if column.width > 0 {
            label.set_label(&truncate(text, budget(column.width)));
        } else {
            label.set_label(text);
        }
    }
}

pub fn widget_for(column: &Column) -> gtk::Widget {
    match column.kind {
        ColumnKind::Text => make_text_cell(column),
        ColumnKind::Icon => make_icon_cell(column),
        ColumnKind::Checkbox => make_checkbox_cell(column),
    }
}

pub fn header_row(columns: &[Column]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_margin_top(6);
    row.set_margin_bottom(6);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_width_request(18);
    row.append(&spacer);

    for column in columns {
        let widget = widget_for(column);
        set_text(&widget, column.title, column);
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            label.add_css_class("caption");
            label.add_css_class("dim-label");
        }
        row.append(&widget);
    }
    row
}

pub fn list_view(columns: &'static [Column]) -> (gtk::ListView, gio::ListStore) {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, list_item| {
        let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_start(12);
        row.set_margin_end(12);
        row.set_margin_top(3);
        row.set_margin_bottom(3);

        let marker = gtk::Label::new(None);
        marker.set_width_request(18);
        marker.add_css_class("dim-label");
        row.append(&marker);

        for column in columns {
            row.append(&widget_for(column));
        }
        list_item.set_child(Some(&row));
    });

    factory.connect_bind(move |_, list_item| {
        let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(object) = list_item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let row_data = object.borrow::<Row>().clone();
        let Some(widget) = list_item.child().and_downcast::<gtk::Box>() else {
            return;
        };

        let mut child = widget.first_child();
        let mut index = 0usize;
        while let Some(widget) = child {
            if index == 0 {
                if let Some(marker) = widget.downcast_ref::<gtk::Label>() {
                    match row_data.marker {
                        Marker::None => marker.set_label(""),
                        Marker::Collapsed => marker.set_label("▸"),
                        Marker::Expanded => marker.set_label("▾"),
                    }
                    marker.set_margin_start(row_data.depth as i32 * 16);
                }
            } else if let Some(cell) = row_data.cells.get(index - 1) {
                match cell {
                    Cell::Text(text) => set_text(&widget, text, &columns[index - 1]),
                    Cell::Icon {
                        name,
                        fallback,
                        tooltip,
                    } => {
                        if let Some(stack) = widget.downcast_ref::<gtk::Stack>() {
                            stack.set_tooltip_text(Some(tooltip));
                            let has_icon =
                                gtk::IconTheme::for_display(&widget.display()).has_icon(name);
                            if has_icon {
                                if let Some(image) =
                                    stack.child_by_name("icon").and_downcast::<gtk::Image>()
                                {
                                    image.set_icon_name(Some(name));
                                }
                                stack.set_visible_child_name("icon");
                            } else {
                                if let Some(label) =
                                    stack.child_by_name("text").and_downcast::<gtk::Label>()
                                {
                                    label.set_label(fallback);
                                }
                                stack.set_visible_child_name("text");
                            }
                        }
                    }
                    Cell::Checkbox(checked) => {
                        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
                            if *checked {
                                label.set_label("☑");
                                label.remove_css_class("dim-label");
                            } else {
                                label.set_label("☐");
                                label.add_css_class("dim-label");
                            }
                        }
                    }
                }
            }
            child = widget.next_sibling();
            index += 1;
        }
    });

    let model = gtk::NoSelection::new(Some(store.clone()));
    let list = gtk::ListView::builder()
        .model(&model)
        .factory(&factory)
        .build();
    (list, store)
}

pub fn replace_rows(store: &gio::ListStore, rows: Vec<Row>) {
    let objects: Vec<glib::BoxedAnyObject> = rows
        .into_iter()
        .map(glib::BoxedAnyObject::new)
        .collect();
    store.splice(0, store.n_items(), &objects);
}

pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .child(child)
        .build()
}
