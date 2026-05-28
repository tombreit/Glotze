use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::gio;

use crate::download::download_dir_display;
use crate::download::progress::{Progress, State};

pub struct DownloadsPage {
    root: gtk::Stack,
    list: gtk::ListBox,
    rows: Rc<RefCell<HashMap<u64, RowWidgets>>>,
}

struct RowWidgets {
    row: adw::ActionRow,
    bar: gtk::ProgressBar,
    icon: gtk::Image,
    open_btn: gtk::Button,
    /// Filled in once the download reaches `Done`; read by the open-folder
    /// button's click handler.
    path: Rc<RefCell<Option<PathBuf>>>,
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
        // note in src/application.rs::orientation_text.
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
        }
    }

    pub fn widget(&self) -> &gtk::Stack {
        &self.root
    }

    /// Apply a progress event from the download manager.
    pub fn apply(&self, p: Progress) {
        // Promote to "list" view as soon as any download appears.
        self.root.set_visible_child_name("list");

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
                    // "12 MB / 34 MB · 35%" — the structure is universal across
                    // locales (numbers, units, percent sign), so we keep the
                    // format string out of the catalogue.
                    entry.row.set_subtitle(&format!(
                        "{} / {} · {:.0}%",
                        human_bytes(*bytes_done),
                        human_bytes(*bytes_total),
                        frac * 100.0
                    ));
                } else {
                    entry.bar.pulse();
                    entry.row.set_subtitle(
                        &gettext("{size} downloaded").replace("{size}", &human_bytes(*bytes_done)),
                    );
                }
                entry.icon.set_icon_name(Some("folder-download-symbolic"));
            }
            State::Done { bytes_total, path } => {
                entry.bar.set_fraction(1.0);
                entry.row.set_subtitle(
                    &gettext("Completed · {size} · {path}")
                        .replace("{size}", &human_bytes(*bytes_total))
                        .replace("{path}", &path.display().to_string()),
                );
                entry.icon.set_icon_name(Some("object-select-symbolic"));
                *entry.path.borrow_mut() = Some(path.clone());
                entry.open_btn.set_visible(true);
            }
            State::Failed { reason } => {
                entry.bar.set_fraction(0.0);
                entry
                    .row
                    .set_subtitle(&gettext("Failed: {reason}").replace("{reason}", reason));
                entry.icon.set_icon_name(Some("dialog-error-symbolic"));
            }
            State::Cancelled => {
                entry.bar.set_fraction(0.0);
                entry.row.set_subtitle(&gettext("Cancelled"));
                entry.icon.set_icon_name(Some("process-stop-symbolic"));
            }
        }
    }

    fn build_row(&self, p: &Progress) -> RowWidgets {
        let row = adw::ActionRow::builder()
            .title(gtk::glib::markup_escape_text(&p.title))
            .subtitle(gettext("Starting…"))
            .build();

        let icon = gtk::Image::from_icon_name("folder-download-symbolic");
        row.add_prefix(&icon);

        let bar = gtk::ProgressBar::builder()
            .fraction(0.0)
            .valign(gtk::Align::Center)
            // A progress bar has no natural width; without a hint it collapses
            // to a few pixels in a row suffix. This is genuine sizing, not the
            // margin/indent guesswork we removed elsewhere.
            .width_request(140)
            .build();
        row.add_suffix(&bar);

        // Reveal-in-file-manager affordance. Hidden until the download
        // completes (see the `Done` arm of `apply`); added last so it sits at
        // the trailing edge of the row.
        let path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
        let open_btn = gtk::Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text(gettext("Show in Files"))
            .valign(gtk::Align::Center)
            .visible(false)
            .css_classes(["flat"])
            .build();
        open_btn.connect_clicked({
            let path = path.clone();
            move |btn| {
                let Some(file_path) = path.borrow().clone() else {
                    return;
                };
                // `open_containing_folder` opens the parent folder and selects
                // the file. Under Flatpak it goes through the OpenURI portal,
                // so no extra filesystem permission is required.
                let launcher = gtk::FileLauncher::new(Some(&gio::File::for_path(&file_path)));
                let window = btn.root().and_downcast::<gtk::Window>();
                launcher.open_containing_folder(window.as_ref(), gio::Cancellable::NONE, |res| {
                    if let Err(e) = res {
                        log::warn!("could not reveal download in file manager: {e}");
                    }
                });
            }
        });
        row.add_suffix(&open_btn);

        self.list.append(&row);
        RowWidgets {
            row,
            bar,
            icon,
            open_btn,
            path,
        }
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
