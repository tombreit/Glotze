pub mod downloads_page;
pub mod format;
pub mod logo;
pub mod results_page;
pub mod row;

/// Maximum width of the search bar and the result lists.
pub const CONTENT_WIDTH: i32 = 860;

/// The layout both pages share: a status page while there's nothing to list,
/// otherwise a boxed list in a width-clamped scroller. Returns the page widget
/// (a stack with the children "empty" and "list") and the list.
pub fn list_page(status: &adw::StatusPage) -> (gtk::Stack, gtk::ListBox) {
    // `selection_mode = None` skips the selection highlight while still letting
    // a row click toggle the expander.
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .valign(gtk::Align::Start)
        .css_classes(["boxed-list"])
        .build();
    let clamp = adw::Clamp::builder()
        .maximum_size(CONTENT_WIDTH)
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

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    stack.add_named(status, Some("empty"));
    stack.add_named(&scroller, Some("list"));
    stack.set_visible_child_name("empty");
    (stack, list)
}
