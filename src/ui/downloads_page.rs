use std::cell::RefCell;
use std::rc::Rc;

use gettextrs::gettext;

use crate::api::models::{Quality, Show};
use crate::download::download_dir_display;
use crate::download::progress::Progress;
use crate::ui::list_page;
use crate::ui::row::{ColumnGroups, ResultRow, RowEvents};

/// Every download of this session as an expanded row, newest last.
pub struct DownloadsPage {
    root: gtk::Stack,
    list: gtk::ListBox,
    rows: RefCell<Vec<Rc<ResultRow>>>,
    /// Shared by every row so the Date / Time / Duration columns line up. The
    /// list only ever grows, so one set lives as long as the page.
    col_groups: ColumnGroups,
    events: RowEvents,
}

impl DownloadsPage {
    pub fn new(events: RowEvents) -> Self {
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
        let (root, list) = list_page(&status);

        Self {
            root,
            list,
            rows: RefCell::new(Vec::new()),
            col_groups: ColumnGroups::new(),
            events,
        }
    }

    pub fn widget(&self) -> &gtk::Stack {
        &self.root
    }

    /// Add a row for the freshly enqueued download `id`.
    pub fn add(&self, show: &Show, quality: Quality, id: u64) {
        let row = ResultRow::new_download(show, quality, id, &self.col_groups, self.events.clone());
        self.list.append(row.widget());
        self.rows.borrow_mut().push(row);
        self.root.set_visible_child_name("list");
    }

    /// Route a progress event to the rows showing that download.
    pub fn apply_progress(&self, p: &Progress) {
        for row in self.rows.borrow().iter() {
            if row.download_id() == Some(p.id) {
                row.apply_progress(&p.state);
            }
        }
    }
}
