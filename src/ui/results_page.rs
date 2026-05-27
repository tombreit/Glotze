use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;

use crate::api::models::Show;
use crate::download::progress::Progress;
use crate::ui::row::{ColumnGroups, ResultRow, RowAction};

type ActionHandler = Rc<RefCell<Option<Box<dyn Fn(Show, RowAction)>>>>;

pub struct ResultsPage {
    root: gtk::Stack,
    list: gtk::ListBox,
    /// Column `SizeGroup`s for the currently-shown rows; kept alive here for as
    /// long as those rows live (a `SizeGroup` drops with its last handle).
    col_groups: RefCell<Option<ColumnGroups>>,
    status: adw::StatusPage,
    rows: Rc<RefCell<Vec<Rc<ResultRow>>>>,
    /// `download_id` -> `show_id`, for routing progress events back to the right row.
    download_routes: Rc<RefCell<HashMap<u64, String>>>,
    /// `show_id` -> current `download_id`, for translating Cancel back to a manager id.
    running: Rc<RefCell<HashMap<String, u64>>>,
    /// User-supplied "action" handler.
    on_action: ActionHandler,
}

impl ResultsPage {
    pub fn new() -> Rc<Self> {
        // A standalone boxed list: `.boxed-list` gives the rounded card, the
        // Clamp keeps it at a comfortable reading width, and the ScrolledWindow
        // makes it scroll. `selection_mode = None` skips the selection
        // highlight while still letting a row click toggle the expander.
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .valign(gtk::Align::Start)
            .css_classes(["boxed-list"])
            .build();
        let clamp = adw::Clamp::builder()
            .maximum_size(860)
            .margin_top(12)
            .margin_bottom(18)
            .margin_start(12)
            .margin_end(12)
            .child(&list)
            .build();
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&clamp)
            .build();

        let status = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title("Search a Mediathek")
            .description("Type into the search field above to find episodes.")
            .build();

        let root = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        root.add_named(&status, Some("empty"));
        root.add_named(&scroller, Some("list"));
        root.set_visible_child_name("empty");

        Rc::new(Self {
            root,
            list,
            col_groups: RefCell::new(None),
            status,
            rows: Rc::new(RefCell::new(Vec::new())),
            download_routes: Rc::new(RefCell::new(HashMap::new())),
            running: Rc::new(RefCell::new(HashMap::new())),
            on_action: Rc::new(RefCell::new(None)),
        })
    }

    pub fn widget(&self) -> &gtk::Stack {
        &self.root
    }

    pub fn show_empty(&self, title: &str, description: &str) {
        self.status.set_title(title);
        self.status.set_description(Some(description));
        self.clear_spinner();
        self.root.set_visible_child_name("empty");
        self.clear_rows();
    }

    /// Like `show_empty`, but puts an `AdwSpinner` in the status page so the
    /// user sees the app is actively waiting on something. Unlike the previous
    /// `AdwSpinnerPaintable` approach, the spinner widget only animates once
    /// it's mapped, so nothing renders before the window is presented.
    pub fn show_loading(&self, title: &str, description: &str) {
        self.status.set_title(title);
        self.status.set_description(Some(description));
        self.status.set_icon_name(None);
        let spinner = adw::Spinner::new();
        spinner.set_size_request(32, 32);
        self.status.set_child(Some(&spinner));
        self.root.set_visible_child_name("empty");
        self.clear_rows();
    }

    fn clear_spinner(&self) {
        // Drop the spinner widget and restore the default search icon.
        self.status.set_child(None::<&gtk::Widget>);
        self.status.set_icon_name(Some("system-search-symbolic"));
    }

    pub fn set_shows(self: &Rc<Self>, shows: &[Show]) {
        self.clear_spinner();
        self.clear_rows();

        if shows.is_empty() {
            self.show_empty("No results", "No episodes matched your search.");
            return;
        }

        // Fresh column groups for this batch of rows; stored so they outlive
        // this call (the rows reference them until the next `clear_rows`).
        let groups = ColumnGroups::new();

        let mut rows = self.rows.borrow_mut();
        for show in shows {
            let result_row = ResultRow::new(show, &groups);

            let on_action = Rc::clone(&self.on_action);
            let show_clone = show.clone();
            result_row.connect_action(move |action| {
                if let Some(cb) = on_action.borrow().as_ref() {
                    cb(show_clone.clone(), action);
                }
            });

            // Single-expansion model: expanding one row collapses the others.
            // Collapsing a row never re-expands anything, so this can't loop.
            let rows_ref = Rc::clone(&self.rows);
            result_row.widget().connect_expanded_notify(move |exp| {
                if !exp.is_expanded() {
                    return;
                }
                let others: Vec<Rc<ResultRow>> = rows_ref
                    .borrow()
                    .iter()
                    .filter(|r| r.widget() != exp)
                    .cloned()
                    .collect();
                for r in others {
                    r.set_expanded(false);
                }
            });

            self.list.append(result_row.widget());
            rows.push(result_row);
        }
        drop(rows);
        *self.col_groups.borrow_mut() = Some(groups);

        self.root.set_visible_child_name("list");
    }

    fn clear_rows(&self) {
        for r in self.rows.borrow().iter() {
            self.list.remove(r.widget());
        }
        self.rows.borrow_mut().clear();
        *self.col_groups.borrow_mut() = None;
        self.download_routes.borrow_mut().clear();
        self.running.borrow_mut().clear();
    }

    /// Register a download with its originating show so progress events can
    /// find the right row and `download_id_for(show_id)` can resolve a cancel.
    pub fn track_download(&self, download_id: u64, show_id: &str) {
        self.download_routes
            .borrow_mut()
            .insert(download_id, show_id.to_string());
        self.running
            .borrow_mut()
            .insert(show_id.to_string(), download_id);
    }

    /// Resolve the currently-running download id for a show, if any.
    pub fn download_id_for(&self, show_id: &str) -> Option<u64> {
        self.running.borrow().get(show_id).copied()
    }

    /// Reverse lookup: which show originated this download? Used by the
    /// progress consumer to forget the running mapping on terminal states.
    pub fn show_id_for_download(&self, download_id: u64) -> Option<String> {
        self.download_routes.borrow().get(&download_id).cloned()
    }

    /// Drop the running-download mapping after a terminal state. The
    /// `download_routes` map stays — it's harmless after the row is gone.
    pub fn forget_running(&self, show_id: &str) {
        self.running.borrow_mut().remove(show_id);
    }

    /// Route a progress event to the matching row, if it's still in the list.
    pub fn apply_progress(&self, p: &Progress) {
        let show_id = {
            let routes = self.download_routes.borrow();
            routes.get(&p.id).cloned()
        };
        let Some(show_id) = show_id else {
            return;
        };
        let rows = self.rows.borrow();
        if let Some(row) = rows.iter().find(|r| r.show_id() == Some(show_id.as_str())) {
            row.apply_progress(&p.state);
        }
    }

    pub fn connect_action<F>(&self, callback: F)
    where
        F: Fn(Show, RowAction) + 'static,
    {
        *self.on_action.borrow_mut() = Some(Box::new(callback));
    }
}
