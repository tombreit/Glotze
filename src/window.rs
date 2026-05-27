use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::api::models::Show;
use crate::api::{Client, Sort};
use crate::download::{Manager, progress::State};
use crate::ui::downloads_page::DownloadsPage;
use crate::ui::results_page::ResultsPage;
use crate::ui::row::RowAction;

const SEARCH_DEBOUNCE_MS: u32 = 300;
const RESULTS_PAGE_SIZE: u32 = 30;

pub struct AppWindow {
    window: adw::ApplicationWindow,
}

impl AppWindow {
    pub fn new(app: &adw::Application) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Glotze")
            .default_width(900)
            .default_height(640)
            .build();

        let results = ResultsPage::new();
        let downloads = Rc::new(DownloadsPage::new());

        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text("Search title or topic…")
            .hexpand(true)
            .build();
        // Sort control: a menu button driven by the stateful `win.sort` action,
        // wired further down once `last_results`/`sort` exist.
        let sort_menu = gio::Menu::new();
        sort_menu.append(Some("Newest first"), Some("win.sort::date-newest"));
        sort_menu.append(Some("Oldest first"), Some("win.sort::date-oldest"));
        sort_menu.append(Some("Longest first"), Some("win.sort::duration-longest"));
        sort_menu.append(Some("Shortest first"), Some("win.sort::duration-shortest"));
        let sort_button = gtk::MenuButton::builder()
            .icon_name("view-sort-descending-symbolic")
            .tooltip_text("Sort results")
            .valign(gtk::Align::Center)
            .menu_model(&sort_menu)
            .build();

        // Fixed search bar above the scrolling results: the entry plus the sort
        // button. Clamped to the same width as the results list below it, with
        // uniform padding so the row isn't edge-to-edge.
        let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        search_row.append(&search_entry);
        search_row.append(&sort_button);
        let search_clamp = adw::Clamp::builder()
            .maximum_size(860)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .child(&search_row)
            .build();

        let search_page_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        search_page_box.append(&search_clamp);
        search_page_box.append(results.widget());
        results.widget().set_vexpand(true);

        let view_stack = adw::ViewStack::new();
        view_stack.add_titled_with_icon(
            &search_page_box,
            Some("search"),
            "Search",
            "system-search-symbolic",
        );
        view_stack.add_titled_with_icon(
            downloads.widget(),
            Some("downloads"),
            "Downloads",
            "folder-download-symbolic",
        );

        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&view_stack));

        let header = build_header_bar(&view_stack);
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&toast_overlay));
        window.set_content(Some(&toolbar));

        install_window_actions(&window, &view_stack, &search_entry);

        // Current sort order plus the last batch of results, so a sort change
        // can re-order what's already on screen without hitting the network.
        let sort = Rc::new(Cell::new(Sort::default()));
        let last_results: Rc<RefCell<Vec<Show>>> = Rc::new(RefCell::new(Vec::new()));
        wire_sort(&window, &results, &last_results, &sort);

        // HTTP client init is fallible (TLS bootstrap). If it fails the app
        // still launches — we just disable search and tell the user why.
        match Client::new() {
            Ok(client) => {
                let manager = Manager::new();
                let generation = Rc::new(Cell::new(0u64));

                wire_search(
                    &search_entry,
                    &results,
                    &toast_overlay,
                    client.clone(),
                    Rc::clone(&generation),
                    Rc::clone(&sort),
                    Rc::clone(&last_results),
                );
                kick_initial_search(
                    client,
                    Rc::clone(&generation),
                    Rc::clone(&results),
                    toast_overlay.clone(),
                    sort.get(),
                    Rc::clone(&last_results),
                );
                wire_row_action(&results, &toast_overlay, &window, Rc::clone(&manager));
                wire_progress_consumer(downloads, &results, &manager, &toast_overlay);
            }
            Err(e) => {
                log::error!("HTTP client init failed: {e:#}");
                search_entry.set_sensitive(false);
                results.show_empty(
                    "Network unavailable",
                    &format!("Glotze couldn't initialise its HTTP client: {e}"),
                );
            }
        }

        Self { window }
    }

    pub fn present(&self) {
        self.window.present();
    }
}

fn build_header_bar(view_stack: &adw::ViewStack) -> adw::HeaderBar {
    let switcher = adw::ViewSwitcher::builder()
        .stack(view_stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();

    let about_button = gtk::Button::builder()
        .icon_name("help-about-symbolic")
        .tooltip_text("About Glotze")
        .action_name("app.about")
        .build();

    let header = adw::HeaderBar::builder().title_widget(&switcher).build();
    header.pack_end(&about_button);
    header
}

fn install_window_actions(
    window: &adw::ApplicationWindow,
    view_stack: &adw::ViewStack,
    search_entry: &gtk::SearchEntry,
) {
    let search_focus = gio::ActionEntry::builder("search-focus")
        .activate(glib::clone!(
            #[weak]
            search_entry,
            move |_win: &adw::ApplicationWindow, _, _| {
                search_entry.grab_focus();
            }
        ))
        .build();
    let show_search = gio::ActionEntry::builder("show-search")
        .activate(glib::clone!(
            #[weak]
            view_stack,
            move |_win: &adw::ApplicationWindow, _, _| {
                view_stack.set_visible_child_name("search");
            }
        ))
        .build();
    let show_downloads = gio::ActionEntry::builder("show-downloads")
        .activate(glib::clone!(
            #[weak]
            view_stack,
            move |_win: &adw::ApplicationWindow, _, _| {
                view_stack.set_visible_child_name("downloads");
            }
        ))
        .build();
    window.add_action_entries([search_focus, show_search, show_downloads]);
}

/// Stateful `win.sort` action backing the sort menu. Changing it re-orders the
/// already-fetched results in place (no refetch); new searches read the current
/// value via `sort`.
fn wire_sort(
    window: &adw::ApplicationWindow,
    results: &Rc<ResultsPage>,
    last_results: &Rc<RefCell<Vec<Show>>>,
    sort: &Rc<Cell<Sort>>,
) {
    let action = gio::ActionEntry::builder("sort")
        .parameter_type(Some(glib::VariantTy::STRING))
        .state(Sort::default().id().to_variant())
        .activate(glib::clone!(
            #[strong]
            results,
            #[strong]
            last_results,
            #[strong]
            sort,
            move |_win: &adw::ApplicationWindow, action, param| {
                let Some(param) = param else {
                    return;
                };
                let Some(id) = param.get::<String>() else {
                    return;
                };
                let Some(new_sort) = Sort::from_id(&id) else {
                    return;
                };
                action.set_state(param);
                sort.set(new_sort);
                let mut shows = last_results.borrow().clone();
                new_sort.apply(&mut shows);
                results.set_shows(&shows);
            }
        ))
        .build();
    window.add_action_entries([action]);
}

fn wire_progress_consumer(
    downloads: Rc<DownloadsPage>,
    results: &Rc<ResultsPage>,
    manager: &Rc<Manager>,
    toast_overlay: &adw::ToastOverlay,
) {
    let rx = manager.progress_rx();
    // `downloads` is owned strongly by the future — nothing else holds a
    // strong ref to the Rust wrapper (view_stack only keeps the GTK widget),
    // and without it the page's rows HashMap would drop the moment
    // `AppWindow::new` returns. The future ends naturally when the Manager
    // drops on window close (rx returns Err), at which point `downloads`
    // is released. results/manager keep their Weak refs so the consumer can
    // detect window-closing without creating an ownership cycle through
    // Manager (which would prevent rx from ever erroring).
    let results_weak: Weak<ResultsPage> = Rc::downgrade(results);
    let manager_weak: Weak<Manager> = Rc::downgrade(manager);
    let toast_weak = toast_overlay.downgrade();

    glib::MainContext::default().spawn_local(async move {
        while let Ok(p) = rx.recv().await {
            let (Some(results), Some(manager), Some(toast)) = (
                results_weak.upgrade(),
                manager_weak.upgrade(),
                toast_weak.upgrade(),
            ) else {
                break;
            };

            let is_terminal = matches!(
                p.state,
                State::Done { .. } | State::Failed { .. } | State::Cancelled
            );
            let title = p.title.clone();
            let id = p.id;
            let state = p.state.clone();

            let show_id_for_cleanup = if is_terminal {
                results.show_id_for_download(id)
            } else {
                None
            };

            results.apply_progress(&p);
            downloads.apply(p);

            if is_terminal {
                if let Some(sid) = show_id_for_cleanup {
                    results.forget_running(&sid);
                }
                manager.forget(id);

                match state {
                    State::Done { .. } => {
                        toast.add_toast(adw::Toast::new(&format!("Download finished: {title}")));
                    }
                    State::Failed { reason } => {
                        toast.add_toast(adw::Toast::new(&format!(
                            "Download failed: {title} ({reason})"
                        )));
                    }
                    State::Cancelled => {
                        toast.add_toast(adw::Toast::new(&format!("Download cancelled: {title}")));
                    }
                    State::Running { .. } => {}
                }
            }
        }
    });
}

fn wire_row_action(
    results: &Rc<ResultsPage>,
    toast_overlay: &adw::ToastOverlay,
    parent: &adw::ApplicationWindow,
    manager: Rc<Manager>,
) {
    let toast_overlay = toast_overlay.clone();
    let parent = parent.clone();
    // Weak ref breaks the self-cycle: this closure is installed on results,
    // so a strong ref here would make ResultsPage permanently un-freeable.
    let results_weak = Rc::downgrade(results);

    results.connect_action(move |show, action| match action {
        RowAction::Download(quality) | RowAction::Retry(quality) => {
            match manager.enqueue(&show, quality) {
                Some(info) => {
                    if let (Some(sid), Some(results)) = (show.id.as_deref(), results_weak.upgrade())
                    {
                        results.track_download(info.id, sid);
                    }
                    let toast = adw::Toast::builder()
                        .title(format!("Download started: {}", info.title))
                        .timeout(3)
                        .build();
                    toast_overlay.add_toast(toast);
                }
                None => {
                    toast_overlay
                        .add_toast(adw::Toast::new("Could not start download (URL missing)."));
                }
            }
        }
        RowAction::Cancel => {
            if let (Some(sid), Some(results)) = (show.id.as_deref(), results_weak.upgrade())
                && let Some(id) = results.download_id_for(sid)
            {
                manager.cancel(id);
            }
        }
        RowAction::Open(path) => {
            let file = gio::File::for_path(&path);
            let launcher = gtk::FileLauncher::new(Some(&file));
            launcher.launch(Some(&parent), gio::Cancellable::NONE, move |result| {
                if let Err(e) = result {
                    log::error!("failed to open file: {e}");
                }
            });
        }
    });
}

fn wire_search(
    search_entry: &gtk::SearchEntry,
    results: &Rc<ResultsPage>,
    toast_overlay: &adw::ToastOverlay,
    client: Client,
    generation: Rc<Cell<u64>>,
    sort: Rc<Cell<Sort>>,
    last_results: Rc<RefCell<Vec<Show>>>,
) {
    let pending_timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    search_entry.connect_search_changed(glib::clone!(
        #[strong]
        results,
        #[strong]
        toast_overlay,
        #[strong]
        client,
        #[strong]
        pending_timer,
        #[strong]
        generation,
        #[strong]
        sort,
        #[strong]
        last_results,
        move |entry| {
            if let Some(id) = pending_timer.borrow_mut().take() {
                id.remove();
            }
            let query = entry.text().to_string();
            let id = glib::timeout_add_local_once(
                Duration::from_millis(u64::from(SEARCH_DEBOUNCE_MS)),
                glib::clone!(
                    #[strong]
                    results,
                    #[strong]
                    toast_overlay,
                    #[strong]
                    client,
                    #[strong]
                    pending_timer,
                    #[strong]
                    generation,
                    #[strong]
                    sort,
                    #[strong]
                    last_results,
                    move || {
                        pending_timer.replace(None);
                        run_search(
                            client.clone(),
                            query.clone(),
                            sort.get(),
                            last_results.clone(),
                            SearchGen::next(&generation),
                            results.clone(),
                            toast_overlay.clone(),
                        );
                    }
                ),
            );
            *pending_timer.borrow_mut() = Some(id);
        }
    ));
}

/// Identifies one in-flight search so a slow response can tell it's been
/// superseded by a newer query — the latest generation wins.
#[derive(Clone)]
struct SearchGen {
    counter: Rc<Cell<u64>>,
    mine: u64,
}

impl SearchGen {
    /// Bump the shared counter and capture the new value as this search's id.
    fn next(counter: &Rc<Cell<u64>>) -> Self {
        let mine = counter.get().wrapping_add(1);
        counter.set(mine);
        Self {
            counter: Rc::clone(counter),
            mine,
        }
    }

    fn is_stale(&self) -> bool {
        self.counter.get() != self.mine
    }
}

/// Cold-start populate with the most recent entries (empty-query search).
fn kick_initial_search(
    client: Client,
    generation: Rc<Cell<u64>>,
    results: Rc<ResultsPage>,
    toast_overlay: adw::ToastOverlay,
    sort: Sort,
    last_results: Rc<RefCell<Vec<Show>>>,
) {
    run_search(
        client,
        String::new(),
        sort,
        last_results,
        SearchGen::next(&generation),
        results,
        toast_overlay,
    );
}

fn run_search(
    client: Client,
    query: String,
    sort: Sort,
    last_results: Rc<RefCell<Vec<Show>>>,
    generation: SearchGen,
    results: Rc<ResultsPage>,
    toast_overlay: adw::ToastOverlay,
) {
    if query.trim().is_empty() {
        results.show_loading("Latest episodes", "Loading the most recent broadcasts…");
    } else {
        results.show_loading("Searching…", &format!("Looking for “{query}”"));
    }

    glib::MainContext::default().spawn_local(async move {
        let q = query.clone();
        let outcome = gio::spawn_blocking(move || client.search(&q, 0, RESULTS_PAGE_SIZE, sort))
            .await
            .map_err(|_| anyhow::anyhow!("search worker panicked"));

        if generation.is_stale() {
            log::debug!("dropping stale search result for {query:?}");
            return;
        }

        match outcome {
            Ok(Ok(shows)) => {
                log::info!("search '{query}' -> {} results", shows.len());
                last_results.borrow_mut().clone_from(&shows);
                render_results(&results, &shows);
            }
            Ok(Err(e)) => {
                log::error!("search '{query}' failed: {e:#}");
                results.show_empty(
                    "Search failed",
                    "Couldn’t reach mediathekviewweb.de — check your connection.",
                );
                toast_overlay.add_toast(adw::Toast::new(&format!("Search failed: {e}")));
            }
            Err(e) => {
                log::error!("spawn_blocking failed: {e:#}");
                toast_overlay.add_toast(adw::Toast::new("Internal error during search"));
            }
        }
    });
}

fn render_results(results: &Rc<ResultsPage>, shows: &[Show]) {
    results.set_shows(shows);
}
