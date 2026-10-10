use std::cell::{Ref, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib;

use crate::api::models::Show;
use crate::download::progress::Progress;
use crate::ui::list_page;
use crate::ui::row::{ColumnGroups, ResultRow, RowEvents};

pub struct ResultsPage {
    root: gtk::Stack,
    list: gtk::ListBox,
    status: adw::StatusPage,
    rows: RefCell<Vec<Rc<ResultRow>>>,
    /// Column `SizeGroup`s for the current rows; kept alive here for as long as
    /// those rows live (a `SizeGroup` drops with its last handle).
    col_groups: RefCell<Option<ColumnGroups>>,
    events: RowEvents,
}

impl ResultsPage {
    pub fn new(events: RowEvents) -> Self {
        let status = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title(gettext("Search a Mediathek"))
            .description(gettext(
                "Type into the search field above to find episodes.",
            ))
            .build();
        let (root, list) = list_page(&status);

        Self {
            root,
            list,
            status,
            rows: RefCell::new(Vec::new()),
            col_groups: RefCell::new(None),
            events,
        }
    }

    pub fn widget(&self) -> &gtk::Stack {
        &self.root
    }

    pub fn rows(&self) -> Ref<'_, Vec<Rc<ResultRow>>> {
        self.rows.borrow()
    }

    pub fn show_empty(&self, title: &str, description: &str) {
        self.show_status(title, description, Some("system-search-symbolic"));
    }

    /// Like `show_empty`, with a spinner in place of the icon.
    pub fn show_loading(&self, title: &str, description: &str) {
        self.show_status(title, description, None);
    }

    fn show_status(&self, title: &str, description: &str, icon: Option<&str>) {
        self.clear_rows();
        self.status.set_title(title);
        self.status.set_description(Some(description));
        self.status.set_icon_name(icon);
        // The spinner only animates while it's mapped, i.e. while visible.
        let spinner = icon.is_none().then(|| {
            adw::Spinner::builder()
                .width_request(32)
                .height_request(32)
                .build()
        });
        self.status.set_child(spinner.as_ref());
        self.root.set_visible_child_name("empty");
    }

    pub fn set_shows(&self, shows: &[Show]) {
        if shows.is_empty() {
            self.show_empty(
                &gettext("No results"),
                &gettext("No episodes matched your search."),
            );
            return;
        }
        self.clear_rows();

        let groups = ColumnGroups::new();
        let mut rows = self.rows.borrow_mut();
        for show in shows {
            let row = ResultRow::new(show, &groups, self.events.clone());
            // Single-expansion model: expanding one row collapses the others.
            row.widget().connect_expanded_notify(glib::clone!(
                #[weak(rename_to = list)]
                self.list,
                move |expander| {
                    if expander.is_expanded() {
                        collapse_others(&list, expander);
                    }
                }
            ));
            self.list.append(row.widget());
            rows.push(row);
        }
        *self.col_groups.borrow_mut() = Some(groups);
        self.root.set_visible_child_name("list");
    }

    fn clear_rows(&self) {
        self.list.remove_all();
        self.rows.borrow_mut().clear();
        *self.col_groups.borrow_mut() = None;
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

fn collapse_others(list: &gtk::ListBox, expanded: &adw::ExpanderRow) {
    let mut child = list.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Some(row) = widget.downcast_ref::<adw::ExpanderRow>()
            && row != expanded
        {
            row.set_expanded(false);
        }
    }
}
