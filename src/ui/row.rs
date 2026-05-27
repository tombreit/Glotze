use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use crate::api::models::{Quality, Show};
use crate::download::progress::State;
use crate::ui::format::{format_date_short, format_duration, format_time};
use crate::ui::logo::channel_logo;

/// What the user just asked the row to do, via the single action button.
#[derive(Debug, Clone)]
pub enum RowAction {
    Download(Quality),
    Cancel,
    Open(PathBuf),
    Retry(Quality),
}

#[derive(Debug, Clone)]
enum ActionState {
    Idle,
    Downloading,
    Done(PathBuf),
    Failed,
}

type ActionHandler = Rc<RefCell<Option<Box<dyn Fn(RowAction)>>>>;

/// A search-result row built on `AdwExpanderRow`: the logo is a prefix, the
/// date/time and terminal-state status icon are suffixes, and the details
/// (description, link, quality picker, download button, progress) live in the
/// expander's revealed area. libadwaita supplies the chevron, the reveal
/// animation, and all the spacing/indentation.
pub struct ResultRow {
    show: Show,
    expander: adw::ExpanderRow,
    /// The unstyled `topic · channel · duration` subtitle, restored after a
    /// transient failure message has been shown in its place.
    subtitle_normal: String,
    status_icon: gtk::Image,

    quality_group: adw::ToggleGroup,
    action_button: gtk::Button,
    action_content: adw::ButtonContent,
    progress_box: gtk::Box,
    progress_bar: gtk::ProgressBar,
    percent_label: gtk::Label,

    action_state: Rc<RefCell<ActionState>>,
    action_handler: ActionHandler,
}

impl ResultRow {
    pub fn new(show: &Show) -> Rc<Self> {
        let expander = adw::ExpanderRow::new();
        expander.set_title(&glib::markup_escape_text(&show.title));
        expander.set_title_lines(1);

        let subtitle_normal = build_subtitle(show);
        if !subtitle_normal.is_empty() {
            expander.set_subtitle(&subtitle_normal);
            expander.set_subtitle_lines(1);
        }

        // ─── Prefix: channel logo ─────────────────────────────────────
        let logo = channel_logo(&show.channel);
        if !show.channel.is_empty() {
            logo.set_tooltip_text(Some(&show.channel));
        }
        expander.add_prefix(&logo);

        // ─── Suffixes: broadcast date/time, then a status icon ────────
        if let Some(ts) = show.timestamp.filter(|t| *t > 0) {
            let right = gtk::Box::new(gtk::Orientation::Vertical, 2);
            right.set_valign(gtk::Align::Center);

            let date_label = gtk::Label::builder()
                .label(format_date_short(ts))
                .css_classes(["caption", "numeric"])
                .halign(gtk::Align::End)
                .tooltip_text("Broadcast date")
                .build();
            let time_label = gtk::Label::builder()
                .label(format_time(ts))
                .css_classes(["caption", "dim-label", "numeric"])
                .halign(gtk::Align::End)
                .tooltip_text("Broadcast start time (Europe/Berlin)")
                .build();
            right.append(&date_label);
            right.append(&time_label);
            expander.add_suffix(&right);
        }

        // Status icon: hidden by default, shown on terminal states
        // (Done = ✓, Failed = ⚠).
        let status_icon = gtk::Image::new();
        status_icon.set_valign(gtk::Align::Center);
        status_icon.set_visible(false);
        expander.add_suffix(&status_icon);

        // ─── Revealed body ────────────────────────────────────────────
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);

        // 1. Description.
        if let Some(desc) = show.description.as_deref().filter(|d| !d.is_empty()) {
            let desc_label = gtk::Label::builder()
                .label(desc)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .xalign(0.0)
                .halign(gtk::Align::Start)
                .selectable(true)
                .build();
            body.append(&desc_label);
        }

        // 2. Sendungsseite link directly below the description.
        if let Some(url) = show.url_website.as_deref().filter(|u| !u.is_empty()) {
            let link = gtk::LinkButton::builder()
                .uri(url)
                .label("Sendungsseite ↗")
                .css_classes(["flat"])
                .halign(gtk::Align::Start)
                .tooltip_text("Open this episode on the broadcaster's page")
                .build();
            body.append(&link);
        }

        // 3. Actions cluster, right-aligned: [spacer] [quality_group] [action_button].
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let action_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        action_spacer.set_hexpand(true);
        actions.append(&action_spacer);

        let quality_group = adw::ToggleGroup::new();
        quality_group.set_tooltip_text(Some("Choose video quality"));
        for (quality, name, label) in [
            (Quality::Low, "low", "Low"),
            (Quality::Medium, "medium", "Medium"),
            (Quality::High, "high", "HD"),
        ] {
            if show.url_for(quality).is_none() {
                continue;
            }
            let toggle = adw::Toggle::builder().name(name).label(label).build();
            quality_group.add(toggle);
        }
        for preferred in ["high", "medium", "low"] {
            if quality_group.toggle_by_name(preferred).is_some() {
                quality_group.set_active_name(Some(preferred));
                break;
            }
        }
        actions.append(&quality_group);

        // Action button: icon + label via adw::ButtonContent.
        let action_content = adw::ButtonContent::builder()
            .icon_name("folder-download-symbolic")
            .label("Download")
            .build();
        let action_button = gtk::Button::builder()
            .child(&action_content)
            .css_classes(["pill", "suggested-action"])
            .build();
        if quality_group.n_toggles() == 0 {
            action_button.set_sensitive(false);
            action_content.set_label("Not available");
        }
        actions.append(&action_button);

        body.append(&actions);

        // 4. Progress bar + percent label.
        let progress_bar = gtk::ProgressBar::builder()
            .hexpand(true)
            .valign(gtk::Align::Center)
            .show_text(false)
            .build();
        let percent_label = gtk::Label::builder()
            .label("")
            .css_classes(["caption", "numeric", "dim-label"])
            .halign(gtk::Align::End)
            .width_chars(4)
            .build();
        let progress_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        progress_box.append(&progress_bar);
        progress_box.append(&percent_label);
        progress_box.set_visible(false);
        body.append(&progress_box);

        // The body is a single, non-activatable row inside the expander; the
        // expander indents and pads it for us.
        let body_row = gtk::ListBoxRow::builder()
            .child(&body)
            .activatable(false)
            .selectable(false)
            .build();
        expander.add_row(&body_row);

        let this = Rc::new(Self {
            show: show.clone(),
            expander,
            subtitle_normal,
            status_icon,
            quality_group,
            action_button,
            action_content,
            progress_box,
            progress_bar,
            percent_label,
            action_state: Rc::new(RefCell::new(ActionState::Idle)),
            action_handler: Rc::new(RefCell::new(None)),
        });

        // Wire the action button to dispatch the current state's RowAction.
        {
            let weak = Rc::downgrade(&this);
            this.action_button.connect_clicked(move |_| {
                if let Some(strong) = weak.upgrade() {
                    strong.dispatch_action();
                }
            });
        }

        this
    }

    pub fn widget(&self) -> &adw::ExpanderRow {
        &self.expander
    }

    pub fn show_id(&self) -> Option<&str> {
        self.show.id.as_deref()
    }

    pub fn set_expanded(&self, expanded: bool) {
        self.expander.set_expanded(expanded);
    }

    pub fn connect_action<F>(&self, callback: F)
    where
        F: Fn(RowAction) + 'static,
    {
        *self.action_handler.borrow_mut() = Some(Box::new(callback));
    }

    fn selected_quality(&self) -> Option<Quality> {
        match self.quality_group.active_name().as_deref() {
            Some("low") => Some(Quality::Low),
            Some("medium") => Some(Quality::Medium),
            Some("high") => Some(Quality::High),
            _ => None,
        }
    }

    fn dispatch_action(&self) {
        let action = {
            let state = self.action_state.borrow();
            match &*state {
                ActionState::Idle => self.selected_quality().map(RowAction::Download),
                ActionState::Downloading => Some(RowAction::Cancel),
                ActionState::Done(path) => Some(RowAction::Open(path.clone())),
                ActionState::Failed => self.selected_quality().map(RowAction::Retry),
            }
        };
        if let Some(action) = action
            && let Some(cb) = self.action_handler.borrow().as_ref()
        {
            cb(action);
        }
    }

    fn set_action_state(&self, new_state: ActionState) {
        let has_qualities = self.quality_group.n_toggles() > 0;
        match &new_state {
            ActionState::Idle => {
                self.action_content
                    .set_icon_name("folder-download-symbolic");
                self.action_content.set_label(if has_qualities {
                    "Download"
                } else {
                    "Not available"
                });
                self.action_button
                    .set_css_classes(&["pill", "suggested-action"]);
                self.action_button.set_sensitive(has_qualities);
                self.set_quality_group_sensitive(true);
            }
            ActionState::Downloading => {
                self.action_content.set_icon_name("process-stop-symbolic");
                self.action_content.set_label("Cancel");
                self.action_button
                    .set_css_classes(&["pill", "destructive-action"]);
                self.action_button.set_sensitive(true);
                self.set_quality_group_sensitive(false);
            }
            ActionState::Done(_) => {
                self.action_content
                    .set_icon_name("media-playback-start-symbolic");
                self.action_content.set_label("Open");
                self.action_button.set_css_classes(&["pill"]);
                self.action_button.set_sensitive(true);
                self.set_quality_group_sensitive(true);
            }
            ActionState::Failed => {
                self.action_content.set_icon_name("view-refresh-symbolic");
                self.action_content.set_label("Retry");
                self.action_button
                    .set_css_classes(&["pill", "suggested-action"]);
                self.action_button.set_sensitive(has_qualities);
                self.set_quality_group_sensitive(true);
            }
        }
        *self.action_state.borrow_mut() = new_state;
    }

    fn set_quality_group_sensitive(&self, sensitive: bool) {
        self.quality_group.set_sensitive(sensitive);
    }

    /// Restore the normal `topic · channel · duration` subtitle after it was
    /// replaced by a failure message.
    fn restore_subtitle(&self) {
        self.expander.set_subtitle(&self.subtitle_normal);
    }

    pub fn apply_progress(&self, state: &State) {
        match state {
            State::Running {
                bytes_done,
                bytes_total,
            } => {
                self.progress_box.set_visible(true);
                if *bytes_total > 0 {
                    let frac = (*bytes_done as f64 / *bytes_total as f64).clamp(0.0, 1.0);
                    self.progress_bar.set_fraction(frac);
                    self.percent_label
                        .set_text(&format!("{:>3.0} %", frac * 100.0));
                } else {
                    self.progress_bar.pulse();
                    self.percent_label.set_text("");
                }
                self.set_action_state(ActionState::Downloading);
                self.restore_subtitle();
                // Header status icon stays hidden during a download.
                self.status_icon.set_visible(false);
            }
            State::Done { path, .. } => {
                self.progress_bar.set_fraction(1.0);
                self.progress_box.set_visible(false);
                self.set_action_state(ActionState::Done(path.clone()));
                self.restore_subtitle();
                self.status_icon.set_icon_name(Some("emblem-ok-symbolic"));
                self.status_icon.set_tooltip_text(Some("Download complete"));
                self.status_icon.remove_css_class("error");
                self.status_icon.add_css_class("success");
                self.status_icon.set_visible(true);
            }
            State::Failed { reason } => {
                self.progress_box.set_visible(false);
                self.set_action_state(ActionState::Failed);
                self.expander
                    .set_subtitle(&glib::markup_escape_text(&format!(
                        "Download failed: {reason}"
                    )));
                self.status_icon
                    .set_icon_name(Some("dialog-error-symbolic"));
                self.status_icon
                    .set_tooltip_text(Some(&format!("Download failed: {reason}")));
                self.status_icon.remove_css_class("success");
                self.status_icon.add_css_class("error");
                self.status_icon.set_visible(true);
            }
            State::Cancelled => {
                self.progress_box.set_visible(false);
                self.progress_bar.set_fraction(0.0);
                self.set_action_state(ActionState::Idle);
                self.restore_subtitle();
                self.status_icon.set_visible(false);
            }
        }
    }
}

/// Build the `topic · channel · duration` subtitle as a single escaped string.
fn build_subtitle(show: &Show) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !show.topic.is_empty() && show.topic != show.title {
        parts.push(glib::markup_escape_text(&show.topic).to_string());
    }
    if !show.channel.is_empty() {
        parts.push(glib::markup_escape_text(&show.channel).to_string());
    }
    if let Some(d) = show.duration.filter(|d| *d > 0) {
        parts.push(glib::markup_escape_text(&format_duration(d)).to_string());
    }
    parts.join(" · ")
}
