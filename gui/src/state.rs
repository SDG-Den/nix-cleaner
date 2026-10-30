use std::cell::{Cell, RefCell};
use std::sync::Arc;

use engine::graph::Graph;
use engine::Index;

pub struct Loaded {
    pub index: Index,
    pub graph: Graph,
}

pub struct State {
    pub loaded: RefCell<Option<Arc<Loaded>>>,
    pub scanning: Cell<bool>,
    pub last_actual: Cell<Option<u64>>,
    pub log: RefCell<String>,
}

pub type SharedState = Arc<State>;

impl State {
    pub fn new() -> SharedState {
        Arc::new(State {
            loaded: RefCell::new(None),
            scanning: Cell::new(false),
            last_actual: Cell::new(None),
            log: RefCell::new(String::new()),
        })
    }

    pub fn set_loaded(&self, index: Index) {
        let graph = Graph::new(&index.paths);
        *self.loaded.borrow_mut() = Some(Arc::new(Loaded { index, graph }));
        self.last_actual.set(None);
    }
}
