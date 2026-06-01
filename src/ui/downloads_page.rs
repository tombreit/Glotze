use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gettextrs::gettext;

use crate::api::models::Show;
use crate::download::download_dir_display;
use crate::download::progress::Progress;
use crate::ui::row::{ColumnGroups, ResultRow, RowAction};

type ActionHandler = Rc<RefCell<Option<Box<dyn Fn(u64, RowAction)>>>>;

pub struct DownloadsPage {
    root: gtk::Stack,
    list: gtk::ListBox,
    /// `download_id` -> the always-expanded row showing that download.
    rows: Rc<RefCell<HashMap<u64, Rc<ResultRow>>>>,
    /// Column `SizeGroup`s shared by every download row so the trailing
    /// Date / Time / Duration labels line up. Created once and reused: downloads
    /// accumulate (the list is never bulk-cleared), so one long-lived set fits
    /// the page lifetime.
    col_groups: ColumnGroups,
    /// User-supplied per-row action handler, keyed by `download_id`.
    on_action: ActionHandler,
}

impl DownloadsPage {
    pub fn new() -> Self {
        // Same standalone boxed-list host as the results page: a `.boxed-list`
        // ListBox in a width-clamped, scrolling column.
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

        // Single physical line on the translatable string so xgettext's C
        // parser sees the same text Rust does at runtime — see the matching
        // note in src/application.rs::welcome_text.
        #[rustfmt::skip]
        let status = adw::StatusPage::builder()
            .icon_name("folder-download-symbolic")
            .title(gettext("No downloads yet"))
            .description(
                gettext("Pick an episode on the Search page and choose a quality. Downloads are saved to {dir}.")
                    .replace("{dir}", &download_dir_display()),
            )
            .build();
        // Resolves against the window-level action, so no ViewStack plumbing.
        let go_search = gtk::Button::builder()
            .label(gettext("Search a Mediathek"))
            .css_classes(["pill", "suggested-action"])
            .halign(gtk::Align::Center)
            .action_name("win.show-search")
            .build();
        status.set_child(Some(&go_search));

        let root = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        root.add_named(&status, Some("empty"));
        root.add_named(&scroller, Some("list"));
        root.set_visible_child_name("empty");

        Self {
            root,
            list,
            rows: Rc::new(RefCell::new(HashMap::new())),
            col_groups: ColumnGroups::new(),
            on_action: Rc::new(RefCell::new(None)),
        }
    }

    pub fn widget(&self) -> &gtk::Stack {
        &self.root
    }

    /// Pre-build the always-expanded row for a freshly enqueued download. Called
    /// at enqueue time (with the originating `Show`, which carries the metadata a
    /// `ResultRow` needs); subsequent `apply` calls drive its progress by id.
    pub fn add_download(&self, download_id: u64, show: &Show) {
        let mut rows = self.rows.borrow_mut();
        if rows.contains_key(&download_id) {
            return;
        }

        let row = ResultRow::new_download(show, &self.col_groups);

        let on_action = Rc::clone(&self.on_action);
        row.connect_action(move |action| {
            if let Some(cb) = on_action.borrow().as_ref() {
                cb(download_id, action);
            }
        });

        self.list.append(row.widget());
        rows.insert(download_id, row);
        drop(rows);

        // Promote to "list" view as soon as the first download appears.
        self.root.set_visible_child_name("list");
    }

    /// Apply a progress event from the download manager to its row.
    pub fn apply(&self, p: Progress) {
        self.root.set_visible_child_name("list");
        if let Some(row) = self.rows.borrow().get(&p.id) {
            row.apply_progress(&p.state);
        }
    }

    pub fn connect_action<F>(&self, callback: F)
    where
        F: Fn(u64, RowAction) + 'static,
    {
        *self.on_action.borrow_mut() = Some(Box::new(callback));
    }
}
