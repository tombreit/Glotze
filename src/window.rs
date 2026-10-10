use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::{gio, glib};

use crate::api::models::Show;
use crate::api::{Client, Sort};
use crate::download::Manager;
use crate::download::progress::{Progress, State};
use crate::ui::CONTENT_WIDTH;
use crate::ui::downloads_page::DownloadsPage;
use crate::ui::results_page::ResultsPage;
use crate::ui::row::{RowAction, RowEvent};

/// Delay before `search-changed` fires, so typing doesn't spam the API.
const SEARCH_DELAY_MS: u32 = 300;
const RESULTS_PAGE_SIZE: u32 = 30;

/// Everything the window's handlers share. It lives as long as the process:
/// Glotze has exactly one window and quits when it closes, so the signal
/// handlers and the two event loops below simply hold strong references.
struct AppWindow {
    window: adw::ApplicationWindow,
    toast_overlay: adw::ToastOverlay,
    search_entry: gtk::SearchEntry,
    results: ResultsPage,
    downloads: DownloadsPage,
    manager: Manager,
    client: Client,
    sort: Cell<Sort>,
    /// The trimmed query of the latest search.
    query: RefCell<String>,
    /// Bumped per search, so a slow response can tell it has been superseded.
    generation: Cell<u64>,
}

/// Build the main window, wire it up, and start the initial "latest episodes"
/// search.
pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let (row_tx, row_rx) = async_channel::unbounded();
    let (manager, progress_rx) = Manager::new();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(gettext("Glotze"))
        .default_width(900)
        .default_height(640)
        .build();
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text(gettext("Search title or topic…"))
        .search_delay(SEARCH_DELAY_MS)
        .hexpand(true)
        .build();
    let results = ResultsPage::new(row_tx.clone());
    let downloads = DownloadsPage::new(row_tx);
    let toast_overlay = adw::ToastOverlay::new();

    // Fixed search bar above the scrolling results, clamped to the same width.
    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    search_row.append(&search_entry);
    search_row.append(&sort_button());
    let search_clamp = adw::Clamp::builder()
        .maximum_size(CONTENT_WIDTH)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .child(&search_row)
        .build();
    let search_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    search_page.append(&search_clamp);
    search_page.append(results.widget());
    results.widget().set_vexpand(true);

    let view_stack = adw::ViewStack::new();
    view_stack.add_titled_with_icon(
        &search_page,
        Some("search"),
        &gettext("Search"),
        "system-search-symbolic",
    );
    view_stack.add_titled_with_icon(
        downloads.widget(),
        Some("downloads"),
        &gettext("Downloads"),
        "folder-download-symbolic",
    );
    toast_overlay.set_child(Some(&view_stack));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header_bar(&view_stack));
    toolbar.set_content(Some(&toast_overlay));
    window.set_content(Some(&toolbar));

    let this = Rc::new(AppWindow {
        window: window.clone(),
        toast_overlay,
        search_entry,
        results,
        downloads,
        manager,
        client: Client::new(),
        sort: Cell::new(Sort::default()),
        query: RefCell::new(String::new()),
        generation: Cell::new(0),
    });

    install_actions(&this, &view_stack);
    // `search-changed` also fires without an actual edit (e.g. once after
    // `search-delay` is set), and for whitespace-only edits; skip those.
    this.search_entry.connect_search_changed(glib::clone!(
        #[strong]
        this,
        move |entry| {
            if entry.text().trim() != *this.query.borrow() {
                this.search();
            }
        }
    ));
    window.connect_close_request(glib::clone!(
        #[strong]
        this,
        move |_| this.on_close_request()
    ));
    glib::spawn_future_local(glib::clone!(
        #[strong]
        this,
        async move {
            while let Ok(event) = row_rx.recv().await {
                this.on_row_event(event);
            }
        }
    ));
    glib::spawn_future_local(glib::clone!(
        #[strong]
        this,
        async move {
            while let Ok(progress) = progress_rx.recv().await {
                this.on_progress(&progress);
            }
        }
    ));

    this.search();
    window
}

impl AppWindow {
    /// Search for the entry's text (empty = latest episodes) in the current
    /// sort order, replacing the result list.
    fn search(self: &Rc<Self>) {
        let query = self.search_entry.text().trim().to_string();
        self.query.replace(query.clone());
        let sort = self.sort.get();
        let generation = self.generation.get() + 1;
        self.generation.set(generation);

        if query.is_empty() {
            self.results.show_loading(
                &gettext("Latest episodes"),
                &gettext("Loading the most recent broadcasts…"),
            );
        } else {
            self.results.show_loading(
                &gettext("Searching…"),
                &gettext("Looking for “{query}”").replace("{query}", &query),
            );
        }

        let this = Rc::clone(self);
        let client = self.client.clone();
        glib::spawn_future_local(async move {
            let q = query.clone();
            let outcome =
                gio::spawn_blocking(move || client.search(&q, RESULTS_PAGE_SIZE, sort)).await;

            if this.generation.get() != generation {
                log::debug!("dropping stale search result for {query:?}");
                return;
            }
            match outcome {
                Ok(Ok(shows)) => {
                    log::info!("search '{query}' -> {} results", shows.len());
                    this.show_results(&shows);
                }
                Ok(Err(e)) => {
                    log::error!("search '{query}' failed: {e:#}");
                    this.results.show_empty(
                        &gettext("Search failed"),
                        &gettext("Couldn’t reach mediathekviewweb.de — check your connection."),
                    );
                    this.toast(
                        &gettext("Search failed: {error}").replace("{error}", &e.to_string()),
                    );
                }
                Err(_) => {
                    log::error!("search worker panicked");
                    this.toast(&gettext("Internal error during search"));
                }
            }
        });
    }

    fn show_results(&self, shows: &[Show]) {
        self.results.set_shows(shows);
        // A show that is downloading right now picks its download up again.
        for row in self.results.rows().iter() {
            if let Some(id) = row.show_id().and_then(|s| self.manager.active_for_show(s)) {
                row.start_download(id);
            }
        }
    }

    fn on_row_event(&self, RowEvent { row, action }: RowEvent) {
        match action {
            RowAction::Download(quality) => match self.manager.enqueue(row.show(), quality) {
                Ok(id) => {
                    row.start_download(id);
                    // A retry on the Downloads page reuses its row; a new
                    // download from the Search page gets one there.
                    if row.is_search_row() {
                        self.downloads.add(row.show(), quality, id);
                    }
                    self.toast_overlay.add_toast(
                        adw::Toast::builder()
                            .title(
                                gettext("Download started: {title}")
                                    .replace("{title}", &row.show().title),
                            )
                            .timeout(3)
                            .build(),
                    );
                }
                Err(e) => self.toast(
                    &gettext("Could not start download: {reason}")
                        .replace("{reason}", &e.to_string()),
                ),
            },
            RowAction::Cancel => {
                if let Some(id) = row.download_id() {
                    self.manager.cancel(id);
                }
            }
            RowAction::Open(path) => open_in_files(&self.window, &path),
        }
    }

    fn on_progress(&self, p: &Progress) {
        // Late events of a download that was already cancelled or finished.
        if !self.manager.is_active(p.id) {
            return;
        }
        self.results.apply_progress(p);
        self.downloads.apply_progress(p);
        if !p.state.is_terminal() {
            return;
        }

        self.manager.forget(p.id);
        let message = match &p.state {
            State::Done { .. } => gettext("Download finished: {title}"),
            State::Failed { reason } => {
                gettext("Download failed: {title} ({reason})").replace("{reason}", reason)
            }
            State::Cancelled => gettext("Download cancelled: {title}"),
            State::Running { .. } => return,
        };
        self.toast(&message.replace("{title}", &p.title));
    }

    /// Guard the window's close against losing running downloads: with any in
    /// flight, ask first. Confirming deletes their partial files and closes;
    /// Glotze never runs in the background.
    fn on_close_request(self: &Rc<Self>) -> glib::Propagation {
        let n = self.manager.active_count();
        if n == 0 {
            return glib::Propagation::Proceed;
        }

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Close Glotze?"))
            .body(
                ngettext(
                    "A download is still in progress. Closing now cancels it and discards the partial file.",
                    "{n} downloads are still in progress. Closing now cancels them and discards the partial files.",
                    n as u32,
                )
                .replace("{n}", &n.to_string()),
            )
            .build();
        dialog.add_response("cancel", &gettext("_Keep Open"));
        dialog.add_response("close", &gettext("_Close Anyway"));
        dialog.set_response_appearance("close", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.connect_response(
            Some("close"),
            glib::clone!(
                #[strong(rename_to = this)]
                self,
                move |_, _| {
                    // With nothing active any more, this close goes through.
                    this.manager.discard_all();
                    this.window.close();
                }
            ),
        );
        dialog.present(Some(&self.window));

        glib::Propagation::Stop
    }

    fn toast(&self, message: &str) {
        self.toast_overlay.add_toast(adw::Toast::new(message));
    }
}

fn install_actions(this: &Rc<AppWindow>, view_stack: &adw::ViewStack) {
    let search_focus = gio::ActionEntry::builder("search-focus")
        .activate(glib::clone!(
            #[strong]
            this,
            move |_: &adw::ApplicationWindow, _, _| {
                this.search_entry.grab_focus();
            }
        ))
        .build();
    let show_search = gio::ActionEntry::builder("show-search")
        .activate(glib::clone!(
            #[strong]
            view_stack,
            move |_: &adw::ApplicationWindow, _, _| view_stack.set_visible_child_name("search")
        ))
        .build();
    let show_downloads = gio::ActionEntry::builder("show-downloads")
        .activate(glib::clone!(
            #[strong]
            view_stack,
            move |_: &adw::ApplicationWindow, _, _| view_stack.set_visible_child_name("downloads")
        ))
        .build();
    // Stateful action behind the sort menu; a change re-runs the search so the
    // server picks the matching episodes, not just a re-ordered page.
    let sort = gio::ActionEntry::builder("sort")
        .parameter_type(Some(glib::VariantTy::STRING))
        .state(Sort::default().id().to_variant())
        .activate(glib::clone!(
            #[strong]
            this,
            move |_: &adw::ApplicationWindow, action, param| {
                if let Some(param) = param
                    && let Some(sort) = param.str().and_then(Sort::from_id)
                {
                    action.set_state(param);
                    this.sort.set(sort);
                    this.search();
                }
            }
        ))
        .build();
    this.window
        .add_action_entries([search_focus, show_search, show_downloads, sort]);
}

fn sort_button() -> gtk::MenuButton {
    let menu = gio::Menu::new();
    menu.append(
        Some(&gettext("Newest first")),
        Some("win.sort::date-newest"),
    );
    menu.append(
        Some(&gettext("Oldest first")),
        Some("win.sort::date-oldest"),
    );
    menu.append(
        Some(&gettext("Longest first")),
        Some("win.sort::duration-longest"),
    );
    menu.append(
        Some(&gettext("Shortest first")),
        Some("win.sort::duration-shortest"),
    );
    gtk::MenuButton::builder()
        .icon_name("view-sort-descending-symbolic")
        .tooltip_text(gettext("Sort results"))
        .valign(gtk::Align::Center)
        .menu_model(&menu)
        .build()
}

fn header_bar(view_stack: &adw::ViewStack) -> adw::HeaderBar {
    let switcher = adw::ViewSwitcher::builder()
        .stack(view_stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();

    let menu = gio::Menu::new();
    menu.append(Some(&gettext("_About Glotze")), Some("app.about"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text(gettext("Main Menu"))
        .menu_model(&menu)
        .primary(true)
        .build();

    let welcome_button = gtk::Button::builder()
        .icon_name("help-about-symbolic")
        .tooltip_text(gettext("How Glotze works"))
        .action_name("app.welcome")
        .build();

    let header = adw::HeaderBar::builder().title_widget(&switcher).build();
    header.pack_end(&menu_button);
    header.pack_end(&welcome_button);
    header
}

/// Reveal a finished download in the file manager, selecting the file. Under
/// Flatpak this goes through the `OpenURI` portal, so no extra filesystem
/// permission is required.
fn open_in_files(parent: &adw::ApplicationWindow, path: &std::path::Path) {
    let launcher = gtk::FileLauncher::new(Some(&gio::File::for_path(path)));
    launcher.open_containing_folder(Some(parent), gio::Cancellable::NONE, |res| {
        if let Err(e) = res {
            log::warn!("could not reveal download in file manager: {e}");
        }
    });
}
