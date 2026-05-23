use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;

use crate::download::progress::{Progress, State};

pub struct DownloadsPage {
    root: gtk::ScrolledWindow,
    list: gtk::ListBox,
    status: adw::StatusPage,
    stack: gtk::Stack,
    rows: Rc<RefCell<HashMap<u64, RowWidgets>>>,
}

struct RowWidgets {
    row: adw::ActionRow,
    bar: gtk::ProgressBar,
    icon: gtk::Image,
}

impl DownloadsPage {
    pub fn new() -> Self {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();

        let group = adw::PreferencesGroup::new();
        group.add(&list);

        let clamp = adw::Clamp::builder()
            .maximum_size(800)
            .tightening_threshold(600)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(12)
            .margin_end(12)
            .child(&group)
            .build();

        let list_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&clamp)
            .build();

        let status = adw::StatusPage::builder()
            .icon_name("folder-download-symbolic")
            .title("No downloads yet")
            .description("Pick an episode from the search page and choose a quality.")
            .build();

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        stack.add_named(&status, Some("empty"));
        stack.add_named(&list_scroll, Some("list"));
        stack.set_visible_child_name("empty");

        let root = gtk::ScrolledWindow::builder().child(&stack).build();
        root.set_propagate_natural_height(true);

        Self {
            root,
            list,
            status,
            stack,
            rows: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

    /// Apply a progress event from the download manager.
    pub fn apply(&self, p: Progress) {
        // Promote to "list" view as soon as any download appears.
        self.stack.set_visible_child_name("list");
        self.status.set_visible(false);

        let mut rows = self.rows.borrow_mut();
        let entry = rows.entry(p.id).or_insert_with(|| self.build_row(&p));
        entry
            .row
            .set_title(&gtk::glib::markup_escape_text(&p.title));
        match &p.state {
            State::Running {
                bytes_done,
                bytes_total,
            } => {
                if *bytes_total > 0 {
                    let frac = (*bytes_done as f64 / *bytes_total as f64).clamp(0.0, 1.0);
                    entry.bar.set_fraction(frac);
                    entry.row.set_subtitle(&format!(
                        "{} / {} · {:.0}%",
                        human_bytes(*bytes_done),
                        human_bytes(*bytes_total),
                        frac * 100.0
                    ));
                } else {
                    entry.bar.pulse();
                    entry
                        .row
                        .set_subtitle(&format!("{} downloaded", human_bytes(*bytes_done)));
                }
                entry.icon.set_icon_name(Some("folder-download-symbolic"));
            }
            State::Done { bytes_total, path } => {
                entry.bar.set_fraction(1.0);
                entry.row.set_subtitle(&format!(
                    "Completed · {} · {}",
                    human_bytes(*bytes_total),
                    path.display()
                ));
                entry.icon.set_icon_name(Some("emblem-ok-symbolic"));
            }
            State::Failed { reason } => {
                entry.bar.set_fraction(0.0);
                entry.row.set_subtitle(&format!("Failed: {reason}"));
                entry.icon.set_icon_name(Some("dialog-error-symbolic"));
            }
            State::Cancelled => {
                entry.bar.set_fraction(0.0);
                entry.row.set_subtitle("Cancelled");
                entry.icon.set_icon_name(Some("process-stop-symbolic"));
            }
        }
    }

    fn build_row(&self, p: &Progress) -> RowWidgets {
        let row = adw::ActionRow::builder()
            .title(gtk::glib::markup_escape_text(&p.title))
            .subtitle("Starting…")
            .build();

        let icon = gtk::Image::from_icon_name("folder-download-symbolic");
        row.add_prefix(&icon);

        let bar = gtk::ProgressBar::builder()
            .fraction(0.0)
            .valign(gtk::Align::Center)
            .width_request(140)
            .build();
        row.add_suffix(&bar);

        self.list.append(&row);
        RowWidgets { row, bar, icon }
    }
}

fn human_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let n = n as f64;
    if n >= GB {
        format!("{:.2} GB", n / GB)
    } else if n >= MB {
        format!("{:.1} MB", n / MB)
    } else if n >= KB {
        format!("{:.0} KB", n / KB)
    } else {
        format!("{n:.0} B")
    }
}
