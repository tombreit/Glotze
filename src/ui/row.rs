use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;

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

/// A search-result row. Holds widget references so we can mutate the header,
/// expansion state, progress, and action button without rebuilding the row.
pub struct ResultRow {
    show: Show,
    row: gtk::ListBoxRow,
    revealer: gtk::Revealer,
    chevron: gtk::Image,
    status_icon: gtk::Image,
    /// Stack switching between the normal `topic · channel · duration` row
    /// and a single-label failure message.
    subtitle_stack: gtk::Stack,
    subtitle_failed_label: gtk::Label,

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
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        outer.set_margin_top(12);
        outer.set_margin_bottom(12);
        outer.set_margin_start(12);
        outer.set_margin_end(12);

        // ─── Header ───────────────────────────────────────────────────
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header.set_valign(gtk::Align::Center);

        let logo = channel_logo(&show.channel);
        logo.set_valign(gtk::Align::Center);
        if !show.channel.is_empty() {
            logo.set_tooltip_text(Some(&show.channel));
        }
        header.append(&logo);

        let centre = gtk::Box::new(gtk::Orientation::Vertical, 2);
        centre.set_hexpand(true);
        centre.set_valign(gtk::Align::Center);

        let title_label = gtk::Label::builder()
            .label(&show.title)
            .css_classes(["heading"])
            .halign(gtk::Align::Start)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .xalign(0.0)
            .single_line_mode(true)
            .build();
        centre.append(&title_label);

        // Subtitle: one Box of labels with their own tooltips, plus a
        // failure-mode label, both inside a Stack.
        let subtitle_normal = build_subtitle_box(show);
        let subtitle_failed_label = gtk::Label::builder()
            .css_classes(["caption", "error"])
            .halign(gtk::Align::Start)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .xalign(0.0)
            .single_line_mode(true)
            .build();
        let subtitle_stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(120)
            .build();
        subtitle_stack.add_named(&subtitle_normal, Some("normal"));
        subtitle_stack.add_named(&subtitle_failed_label, Some("failed"));
        subtitle_stack.set_visible_child_name("normal");
        centre.append(&subtitle_stack);

        header.append(&centre);

        if let Some(ts) = show.timestamp.filter(|t| *t > 0) {
            let right = gtk::Box::new(gtk::Orientation::Vertical, 2);
            right.set_valign(gtk::Align::Center);
            right.set_halign(gtk::Align::End);

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
            header.append(&right);
        }

        // Status icon: hidden by default. Only appears on terminal states
        // (Done = ✓, Failed = ⚠). The download glyph itself lives on the
        // action button now.
        let status_icon = gtk::Image::new();
        status_icon.set_valign(gtk::Align::Center);
        status_icon.set_pixel_size(16);
        status_icon.set_visible(false);
        header.append(&status_icon);

        let chevron = gtk::Image::from_icon_name("pan-end-symbolic");
        chevron.set_valign(gtk::Align::Center);
        chevron.set_pixel_size(16);
        chevron.add_css_class("dim-label");
        chevron.set_tooltip_text(Some("Show details"));
        header.append(&chevron);

        outer.append(&header);

        // ─── Revealer body ────────────────────────────────────────────
        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(180)
            .reveal_child(false)
            .build();

        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.set_margin_top(12);
        body.set_margin_start(52);
        body.set_margin_end(8);

        // 1. Description.
        if let Some(desc) = show.description.as_deref().filter(|d| !d.is_empty()) {
            let desc_label = gtk::Label::builder()
                .label(desc)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .xalign(0.0)
                .selectable(true)
                .build();
            desc_label.set_halign(gtk::Align::Start);
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

        revealer.set_child(Some(&body));
        outer.append(&revealer);

        let row = gtk::ListBoxRow::builder()
            .child(&outer)
            .activatable(true)
            .selectable(false)
            .build();

        let this = Rc::new(Self {
            show: show.clone(),
            row,
            revealer,
            chevron,
            status_icon,
            subtitle_stack,
            subtitle_failed_label,
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

    pub fn widget(&self) -> &gtk::ListBoxRow {
        &self.row
    }

    pub fn show_id(&self) -> Option<&str> {
        self.show.id.as_deref()
    }

    pub fn is_expanded(&self) -> bool {
        self.revealer.reveals_child()
    }

    pub fn set_expanded(&self, expanded: bool) {
        self.revealer.set_reveal_child(expanded);
        self.chevron.set_icon_name(Some(if expanded {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        }));
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
                self.subtitle_stack.set_visible_child_name("normal");
                // Header status icon stays hidden during a download.
                self.status_icon.set_visible(false);
            }
            State::Done { path, .. } => {
                self.progress_bar.set_fraction(1.0);
                self.progress_box.set_visible(false);
                self.set_action_state(ActionState::Done(path.clone()));
                self.subtitle_stack.set_visible_child_name("normal");
                self.status_icon.set_icon_name(Some("emblem-ok-symbolic"));
                self.status_icon.set_tooltip_text(Some("Download complete"));
                self.status_icon.remove_css_class("error");
                self.status_icon.add_css_class("success");
                self.status_icon.set_visible(true);
            }
            State::Failed { reason } => {
                self.progress_box.set_visible(false);
                self.set_action_state(ActionState::Failed);
                self.subtitle_failed_label
                    .set_text(&format!("Download failed: {reason}"));
                self.subtitle_stack.set_visible_child_name("failed");
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
                self.subtitle_stack.set_visible_child_name("normal");
                self.status_icon.set_visible(false);
            }
        }
    }
}

/// Build the "topic · channel · duration" subtitle as a horizontal Box where
/// each metadata token is its own Label with a tooltip explaining what it is.
fn build_subtitle_box(show: &Show) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_halign(gtk::Align::Start);

    let mut needs_separator = false;
    let mk_dot = || {
        gtk::Label::builder()
            .label("·")
            .css_classes(["caption", "dim-label"])
            .build()
    };

    if !show.topic.is_empty() && show.topic != show.title {
        let lbl = gtk::Label::builder()
            .label(&show.topic)
            .css_classes(["caption", "dim-label"])
            .tooltip_text("Show or series this episode belongs to")
            .halign(gtk::Align::Start)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .xalign(0.0)
            .build();
        row.append(&lbl);
        needs_separator = true;
    }

    if !show.channel.is_empty() {
        if needs_separator {
            row.append(&mk_dot());
        }
        let lbl = gtk::Label::builder()
            .label(&show.channel)
            .css_classes(["caption", "dim-label"])
            .tooltip_text("Broadcaster")
            .build();
        row.append(&lbl);
        needs_separator = true;
    }

    if let Some(d) = show.duration.filter(|d| *d > 0) {
        if needs_separator {
            row.append(&mk_dot());
        }
        let lbl = gtk::Label::builder()
            .label(format_duration(d))
            .css_classes(["caption", "dim-label", "numeric"])
            .tooltip_text("Length of the video (h:mm:ss or mm:ss)")
            .build();
        row.append(&lbl);
    }

    row
}
