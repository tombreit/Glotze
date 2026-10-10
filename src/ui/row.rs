use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::api::models::{Quality, Show};
use crate::download::progress::State;
use crate::ui::format::{format_date_short, format_duration, format_time};
use crate::ui::logo::channel_logo;

/// Horizontal `SizeGroup`s shared by every row so the trailing Date / Time /
/// Duration labels line up into straight columns. One group per column; the
/// caller owns them for as long as the rows live (see `ResultsPage`).
pub struct ColumnGroups {
    pub date: gtk::SizeGroup,
    pub time: gtk::SizeGroup,
    pub duration: gtk::SizeGroup,
}

impl ColumnGroups {
    pub fn new() -> Self {
        let mk = || gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
        Self {
            date: mk(),
            time: mk(),
            duration: mk(),
        }
    }
}

/// What the user just asked the row to do, via the single action button.
#[derive(Debug, Clone)]
pub enum RowAction {
    /// Start a download in this quality — also how a failed or cancelled one
    /// is retried.
    Download(Quality),
    Cancel,
    Open(PathBuf),
}

/// A click on a row's action button, handled by the window's event loop.
pub struct RowEvent {
    pub row: Rc<ResultRow>,
    pub action: RowAction,
}

pub type RowEvents = async_channel::Sender<RowEvent>;

#[derive(Debug, Clone, PartialEq)]
enum ActionState {
    Idle,
    Downloading,
    Done(PathBuf),
    Failed(String),
}

/// Where a row lives. A `Search` row is collapsible and offers a quality picker
/// plus a Download button; a `Download` row (on the Downloads page) is the same
/// widget, expanded, with its quality fixed to the one it was enqueued with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowMode {
    Search,
    Download,
}

/// A search-result row built on `AdwExpanderRow`: the logo is a prefix; the
/// series is the title with the episode as the subtitle; the broadcast Date,
/// Time, and Duration are right-aligned suffix columns (kept in line across
/// rows by shared `SizeGroup`s) followed by a terminal-state status icon; and
/// the details (description, link, quality picker, download button, progress)
/// live in the expander's revealed area. libadwaita supplies the chevron, the
/// reveal animation, and all the spacing/indentation.
pub struct ResultRow {
    show: Show,
    mode: RowMode,
    expander: adw::ExpanderRow,
    /// The episode subtitle (or empty), restored after a failure message has
    /// been shown in its place.
    subtitle_normal: String,
    status_icon: gtk::Image,

    quality_group: adw::ToggleGroup,
    action_button: gtk::Button,
    action_content: adw::ButtonContent,
    progress_box: gtk::Box,
    progress_bar: gtk::ProgressBar,
    percent_label: gtk::Label,

    action_state: RefCell<ActionState>,
    /// The download this row is showing, if any.
    download_id: Cell<Option<u64>>,
    events: RowEvents,
}

impl ResultRow {
    /// A collapsible search-result row with a quality picker and Download button.
    pub fn new(show: &Show, cols: &ColumnGroups, events: RowEvents) -> Rc<Self> {
        Self::build(show, cols, events, RowMode::Search)
    }

    /// The same row for the Downloads page: expanded, showing download `id`
    /// of `show` in `quality`. Retrying re-downloads that same quality.
    pub fn new_download(
        show: &Show,
        quality: Quality,
        id: u64,
        cols: &ColumnGroups,
        events: RowEvents,
    ) -> Rc<Self> {
        let row = Self::build(show, cols, events, RowMode::Download);
        // The picker isn't shown on this page, but its active toggle is what
        // a retry reads.
        row.quality_group.set_active_name(Some(quality.tag()));
        row.start_download(id);
        row.expander.set_expanded(true);
        row
    }

    fn build(show: &Show, cols: &ColumnGroups, events: RowEvents, mode: RowMode) -> Rc<Self> {
        let expander = adw::ExpanderRow::new();

        // Series is the headline; the episode sits below it. The channel is
        // already conveyed by the logo, so it stays out of the text.
        let (headline, subline) = headline_subline(show);
        expander.set_title(&headline);
        expander.set_title_lines(1);
        expander.set_subtitle_lines(1);
        let subtitle_normal = subline.unwrap_or_default();

        // ─── Prefix: channel logo ─────────────────────────────────────
        let logo = channel_logo(&show.channel);
        if !show.channel.is_empty() {
            logo.set_tooltip_text(Some(&show.channel));
        }
        expander.add_prefix(&logo);

        // ─── Suffix columns: Date · Time · Duration, then a status icon ───
        // Each label is right-aligned and joined to a shared SizeGroup so the
        // three form straight columns across every row. They're always added
        // (blank when the datum is missing) so the columns never shift.
        let ts = show.timestamp.filter(|t| *t > 0);
        let column = |text: String, tooltip: String, group: &gtk::SizeGroup| {
            let label = gtk::Label::builder()
                .label(text)
                .css_classes(["caption", "numeric", "dim-label"])
                .halign(gtk::Align::End)
                .xalign(1.0)
                .valign(gtk::Align::Center)
                .tooltip_text(tooltip)
                .build();
            group.add_widget(&label);
            label
        };
        let date_label = column(
            ts.map(format_date_short).unwrap_or_default(),
            gettext("Broadcast date"),
            &cols.date,
        );
        let time_label = column(
            ts.map(format_time).unwrap_or_default(),
            gettext("Broadcast start time (Europe/Berlin)"),
            &cols.time,
        );
        let duration_label = column(
            show.duration
                .filter(|d| *d > 0)
                .map(format_duration)
                .unwrap_or_default(),
            gettext("Duration"),
            &cols.duration,
        );

        // Status icon: shown on terminal states (Done = ✓, Failed = ⚠).
        let status_icon = gtk::Image::builder()
            .valign(gtk::Align::Center)
            .visible(false)
            .build();

        // A single suffix box fixes the visual order (per-suffix packing reverses
        // it) and its `spacing` sets the columns apart.
        let meta = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        meta.set_valign(gtk::Align::Center);
        meta.append(&date_label);
        meta.append(&time_label);
        meta.append(&duration_label);
        meta.append(&status_icon);
        expander.add_suffix(&meta);

        // ─── Revealed body ────────────────────────────────────────────
        // Two self-padding rows: the description as an AdwActionRow (padded by
        // its own header) and a controls row whose pieces live in `.toolbar`
        // boxes (6px builtin padding + spacing). No manual margins anywhere.

        // 1. Description row.
        if let Some(desc) = show.description.as_deref().filter(|d| !d.is_empty()) {
            let desc_row = adw::ActionRow::builder()
                .title(desc)
                .use_markup(false)
                .title_lines(0)
                .title_selectable(true)
                .activatable(false)
                .selectable(false)
                .build();
            expander.add_row(&desc_row);
        }

        // 2. Controls: Website on the left, quality + download on the right.
        let actions = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["toolbar"])
            .build();

        if let Some(url) = show.url_website.as_deref().filter(|u| !u.is_empty()) {
            actions.append(&website_button(url));
        }

        let action_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        action_spacer.set_hexpand(true);
        actions.append(&action_spacer);

        let quality_group = adw::ToggleGroup::new();
        quality_group.add_css_class("round");
        quality_group.set_tooltip_text(Some(&gettext("Choose video quality")));
        for quality in Quality::ALL {
            if show.url_for(quality).is_none() {
                continue;
            }
            let label = match quality {
                Quality::Low => gettext("Low"),
                Quality::Medium => gettext("Medium"),
                // "HD" is a worldwide technical marker, kept untranslated even
                // in languages with a native term — matches the rest of the
                // GNOME video ecosystem (Showtime, Decibels).
                Quality::High => "HD".to_string(),
            };
            quality_group.add(
                adw::Toggle::builder()
                    .name(quality.tag())
                    .label(&label)
                    .build(),
            );
        }
        // Preselect the best quality on offer.
        if let Some(best) = Quality::ALL
            .into_iter()
            .rev()
            .find(|q| quality_group.toggle_by_name(q.tag()).is_some())
        {
            quality_group.set_active_name(Some(best.tag()));
        }
        // Only show the picker when there's an actual choice to offer, and
        // only on the Search page.
        if mode == RowMode::Search && quality_group.n_toggles() > 0 {
            actions.append(&quality_group);
        }

        // Action button: icon + label via adw::ButtonContent; its look follows
        // the action state (see `render`).
        let action_content = adw::ButtonContent::new();
        let action_button = gtk::Button::builder().child(&action_content).build();
        actions.append(&action_button);

        // Progress bar + percent label, in their own `.toolbar` box so they're
        // padded to match the actions above; hidden until a download runs.
        let progress_bar = gtk::ProgressBar::builder()
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        let percent_label = gtk::Label::builder()
            .css_classes(["caption", "numeric", "dim-label"])
            .halign(gtk::Align::End)
            .width_chars(4)
            .build();
        let progress_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["toolbar"])
            .visible(false)
            .build();
        progress_box.append(&progress_bar);
        progress_box.append(&percent_label);

        // Both `.toolbar` boxes share one non-activatable revealed row; the
        // always-present actions box keeps it from collapsing when progress
        // is hidden.
        let controls = gtk::Box::new(gtk::Orientation::Vertical, 0);
        controls.append(&actions);
        controls.append(&progress_box);
        let controls_row = gtk::ListBoxRow::builder()
            .child(&controls)
            .activatable(false)
            .selectable(false)
            .build();
        expander.add_row(&controls_row);

        let this = Rc::new(Self {
            show: show.clone(),
            mode,
            expander,
            subtitle_normal,
            status_icon,
            quality_group,
            action_button,
            action_content,
            progress_box,
            progress_bar,
            percent_label,
            action_state: RefCell::new(ActionState::Idle),
            download_id: Cell::new(None),
            events,
        });
        this.render(&ActionState::Idle);

        let weak = Rc::downgrade(&this);
        this.action_button.connect_clicked(move |_| {
            if let Some(row) = weak.upgrade() {
                row.on_clicked();
            }
        });

        this
    }

    pub fn widget(&self) -> &adw::ExpanderRow {
        &self.expander
    }

    pub fn show(&self) -> &Show {
        &self.show
    }

    pub fn show_id(&self) -> Option<&str> {
        self.show.id.as_deref()
    }

    pub fn is_search_row(&self) -> bool {
        self.mode == RowMode::Search
    }

    pub fn download_id(&self) -> Option<u64> {
        self.download_id.get()
    }

    /// Show download `id` on this row from now on: the button becomes Cancel
    /// and progress events carrying `id` are applied here.
    pub fn start_download(&self, id: u64) {
        self.download_id.set(Some(id));
        self.progress_bar.set_fraction(0.0);
        self.percent_label.set_text("");
        self.set_action_state(ActionState::Downloading);
    }

    pub fn apply_progress(&self, state: &State) {
        match state {
            State::Running {
                bytes_done,
                bytes_total,
            } => {
                self.set_action_state(ActionState::Downloading);
                if *bytes_total > 0 {
                    let frac = (*bytes_done as f64 / *bytes_total as f64).clamp(0.0, 1.0);
                    self.progress_bar.set_fraction(frac);
                    self.percent_label
                        .set_text(&format!("{:>3.0} %", frac * 100.0));
                } else {
                    self.progress_bar.pulse();
                }
            }
            State::Done { path } => self.set_action_state(ActionState::Done(path.clone())),
            State::Failed { reason } => self.set_action_state(ActionState::Failed(reason.clone())),
            State::Cancelled => self.set_action_state(ActionState::Idle),
        }
    }

    fn on_clicked(self: Rc<Self>) {
        let action = match &*self.action_state.borrow() {
            ActionState::Idle | ActionState::Failed(_) => {
                self.selected_quality().map(RowAction::Download)
            }
            ActionState::Downloading => Some(RowAction::Cancel),
            ActionState::Done(path) => Some(RowAction::Open(path.clone())),
        };
        if let Some(action) = action {
            let _ = self.events.try_send(RowEvent {
                row: Rc::clone(&self),
                action,
            });
        }
    }

    fn selected_quality(&self) -> Option<Quality> {
        let active = self.quality_group.active_name()?;
        Quality::ALL.into_iter().find(|q| q.tag() == active)
    }

    /// Switch state, touching the widgets only on an actual change — progress
    /// events arrive several times a second.
    fn set_action_state(&self, new_state: ActionState) {
        if *self.action_state.borrow() == new_state {
            return;
        }
        self.render(&new_state);
        *self.action_state.borrow_mut() = new_state;
    }

    fn render(&self, state: &ActionState) {
        let can_download = self.quality_group.n_toggles() > 0;
        let (icon, label, css, sensitive) = match state {
            ActionState::Idle if can_download => (
                "folder-download-symbolic",
                gettext("Download"),
                &["pill", "suggested-action"][..],
                true,
            ),
            ActionState::Idle => (
                "folder-download-symbolic",
                gettext("Not available"),
                &["pill", "suggested-action"][..],
                false,
            ),
            ActionState::Downloading => (
                "process-stop-symbolic",
                gettext("Cancel"),
                &["pill", "destructive-action"][..],
                true,
            ),
            // Reveals the file in the file manager, so the folder icon — not
            // a play icon — matches what happens.
            ActionState::Done(_) => ("folder-open-symbolic", gettext("Open"), &["pill"][..], true),
            ActionState::Failed(_) => (
                "view-refresh-symbolic",
                gettext("Retry"),
                &["pill", "suggested-action"][..],
                can_download,
            ),
        };
        self.action_content.set_icon_name(icon);
        self.action_content.set_label(&label);
        self.action_button.set_css_classes(css);
        self.action_button.set_sensitive(sensitive);

        let downloading = *state == ActionState::Downloading;
        self.quality_group.set_sensitive(!downloading);
        self.progress_box.set_visible(downloading);

        // Header: a status icon for terminal states; a failure also replaces
        // the episode subtitle with the reason until the next state change.
        let mut subtitle = self.subtitle_normal.clone();
        match state {
            ActionState::Done(_) => {
                self.show_status_icon(
                    "emblem-ok-symbolic",
                    "success",
                    &gettext("Download complete"),
                );
            }
            ActionState::Failed(reason) => {
                let msg = gettext("Download failed: {reason}").replace("{reason}", reason);
                self.show_status_icon("dialog-error-symbolic", "error", &msg);
                subtitle = glib::markup_escape_text(&msg).to_string();
            }
            ActionState::Idle | ActionState::Downloading => self.status_icon.set_visible(false),
        }
        self.expander.set_subtitle(&subtitle);
    }

    fn show_status_icon(&self, icon: &str, css_class: &str, tooltip: &str) {
        self.status_icon.set_icon_name(Some(icon));
        self.status_icon.set_css_classes(&[css_class]);
        self.status_icon.set_tooltip_text(Some(tooltip));
        self.status_icon.set_visible(true);
    }
}

/// A subtle flat button that opens the episode's page on the broadcaster's site.
fn website_button(url: &str) -> gtk::Button {
    let content = adw::ButtonContent::builder()
        .icon_name("adw-external-link-symbolic")
        .label(gettext("Website"))
        .build();
    let button = gtk::Button::builder()
        .child(&content)
        .css_classes(["flat"])
        .tooltip_text(gettext("Open this episode on the broadcaster's page"))
        .build();
    let uri = url.to_string();
    button.connect_clicked(move |btn| {
        let window = btn.root().and_downcast::<gtk::Window>();
        gtk::UriLauncher::new(&uri).launch(window.as_ref(), gio::Cancellable::NONE, |res| {
            if let Err(e) = res {
                log::warn!("could not open website: {e}");
            }
        });
    });
    button
}

/// Split a show into its bold headline and dim subline, both markup-escaped.
/// When the series (`topic`) is present and distinct it leads, with the episode
/// (`title`) underneath; otherwise the title stands alone with no subline.
fn headline_subline(show: &Show) -> (String, Option<String>) {
    let topic = show.topic.trim();
    let title = show.title.trim();
    if !topic.is_empty() && topic != title {
        let subline = (!title.is_empty()).then(|| glib::markup_escape_text(title).to_string());
        (glib::markup_escape_text(topic).to_string(), subline)
    } else {
        (glib::markup_escape_text(title).to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::headline_subline;
    use crate::api::models::Show;

    fn show(topic: &str, title: &str) -> Show {
        serde_json::from_value(serde_json::json!({ "topic": topic, "title": title, "channel": "" }))
            .unwrap()
    }

    #[test]
    fn series_leads_episode_follows() {
        assert_eq!(
            headline_subline(&show("Tatort", "Folge 1")),
            ("Tatort".into(), Some("Folge 1".into()))
        );
    }

    #[test]
    fn title_alone_when_series_missing_or_same() {
        assert_eq!(headline_subline(&show("", "Doku")), ("Doku".into(), None));
        assert_eq!(
            headline_subline(&show("Doku", "Doku")),
            ("Doku".into(), None)
        );
    }

    #[test]
    fn markup_is_escaped() {
        assert_eq!(
            headline_subline(&show("Tom & Jerry", "<b>")),
            ("Tom &amp; Jerry".into(), Some("&lt;b&gt;".into()))
        );
    }
}
