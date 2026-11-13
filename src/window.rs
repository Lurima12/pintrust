use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::clone;
use webkit6::prelude::*;

use crate::config;
use crate::settings::{NavItem, NavPlacement, Settings};
use crate::theme_injector::ThemeInjector;
use crate::webview;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Window {
        pub webview: RefCell<Option<webkit6::WebView>>,
        pub _theme_injector: RefCell<Option<Rc<ThemeInjector>>>,
        pub progress: RefCell<Option<gtk::ProgressBar>>,
        pub stack: RefCell<Option<gtk::Stack>>,
        pub status_page: RefCell<Option<adw::StatusPage>>,
        pub failed_uri: RefCell<Option<String>>,
        pub header: RefCell<Option<adw::HeaderBar>>,
        pub overlay: RefCell<Option<gtk::Overlay>>,
        pub nav: RefCell<Option<gtk::Box>>,
        pub nav_buttons: RefCell<Vec<(crate::settings::NavItem, gtk::Button)>>,
        pub settings: RefCell<crate::settings::Settings>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "PintRustWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;
    }

    impl glib::subclass::prelude::ObjectImpl for Window {}
    impl gtk::subclass::prelude::WidgetImpl for Window {}
    impl gtk::subclass::prelude::WindowImpl for Window {}
    impl gtk::subclass::prelude::ApplicationWindowImpl for Window {}
    impl adw::subclass::prelude::AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                    gtk::Native, gtk::Root, gtk::ShortcutManager,
                    gio::ActionGroup, gio::ActionMap;
}

impl Window {
    pub fn new(app: &adw::Application) -> Self {
        let window: Self = glib::Object::builder()
            .property("default-width", 1200)
            .property("default-height", 860)
            .build();

        window.set_application(Some(app));
        window.set_title(Some(config::APP_NAME));
        window.setup_ui(app);
        window
    }

    fn setup_ui(&self, app: &adw::Application) {
        self.imp().settings.replace(Settings::load());

        let header = adw::HeaderBar::new();
        header.add_css_class("pintrust-header");

        let menu = gio::Menu::new();
        menu.append(Some("Preferences"), Some("win.preferences"));
        menu.append(
            Some("Open Current Page in Browser"),
            Some("win.open-in-browser"),
        );
        menu.append(Some("About PintRust"), Some("win.about"));
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .tooltip_text("Main Menu")
            .primary(true)
            .build();
        header.pack_end(&menu_button);

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);

        let (webview, injector) = webview::build_webview(app);
        self.imp().webview.replace(Some(webview.clone()));
        self.imp()._theme_injector.replace(Some(injector));

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&webview));

        let progress = gtk::ProgressBar::new();
        progress.set_halign(gtk::Align::Fill);
        progress.set_valign(gtk::Align::Start);
        progress.set_show_text(false);
        progress.add_css_class("pintrust-progress");
        progress.set_visible(false);
        overlay.add_overlay(&progress);
        self.imp().progress.replace(Some(progress.clone()));

        // The navigation buttons can float in any corner of the page or stay in
        // the title bar; the placement and the order are user preferences. All
        // four buttons drive window actions, so their enabled state still
        // follows the history.
        let nav = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        let mut nav_buttons = Vec::with_capacity(NavItem::ALL.len());
        for item in NavItem::ALL {
            nav_buttons.push((item, nav_button(item)));
        }
        self.imp().nav_buttons.replace(nav_buttons);

        self.imp().nav.replace(Some(nav));
        self.imp().header.replace(Some(header));
        self.imp().overlay.replace(Some(overlay.clone()));
        self.apply_navigation_settings();

        let status_page = adw::StatusPage::builder()
            .icon_name("network-offline-symbolic")
            .title("Unable to Load Pinterest")
            .description("Check your internet connection and try again.")
            .build();

        let retry = gtk::Button::with_label("Try Again");
        retry.add_css_class("suggested-action");
        retry.add_css_class("pill");
        retry.connect_clicked(clone!(
            #[weak(rename_to = window)]
            self,
            move |_| window.retry()
        ));

        let browser = gtk::Button::with_label("Open in Browser");
        browser.add_css_class("pill");
        browser.connect_clicked(clone!(
            #[weak(rename_to = window)]
            self,
            move |_| window.open_current_in_browser()
        ));

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        actions.set_halign(gtk::Align::Center);
        actions.append(&retry);
        actions.append(&browser);
        status_page.set_child(Some(&actions));

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_transition_duration(150);
        stack.add_named(&overlay, Some("web"));
        stack.add_named(&status_page, Some("error"));
        stack.set_visible_child_name("web");
        self.imp().stack.replace(Some(stack.clone()));
        self.imp().status_page.replace(Some(status_page));

        toolbar_view.set_content(Some(&stack));
        self.set_content(Some(&toolbar_view));

        let progress_for_load = progress;
        webview.connect_load_changed(clone!(
            #[weak(rename_to = window)]
            self,
            move |_webview, event| {
                match event {
                    webkit6::LoadEvent::Started => {
                        window.set_page("web");
                        progress_for_load.set_visible(true);
                        progress_for_load.set_fraction(0.0);
                    }
                    webkit6::LoadEvent::Finished => {
                        progress_for_load.set_visible(false);
                        progress_for_load.set_fraction(0.0);
                    }
                    _ => {}
                }
                window.update_navigation_state();
            }
        ));

        webview.connect_estimated_load_progress_notify(clone!(
            #[weak(rename_to = window)]
            self,
            move |webview| {
                if let Some(progress) = window.imp().progress.borrow().as_ref() {
                    progress.set_fraction(webview.estimated_load_progress());
                }
            }
        ));

        let failed_window = self.downgrade();
        webview.connect_load_failed(move |_webview, _event, uri, error| {
            let Some(window) = failed_window.upgrade() else {
                return false;
            };
            if error.matches(webkit6::NetworkError::Cancelled) {
                return false;
            }
            window.show_error(uri, error);
            true
        });

        webview.connect_uri_notify(clone!(
            #[weak(rename_to = window)]
            self,
            move |_webview| window.update_navigation_state()
        ));

        webview.connect_title_notify(clone!(
            #[weak(rename_to = window)]
            self,
            move |webview| {
                if let Some(title) = webview.title() {
                    window.set_title(Some(&title));
                }
            }
        ));

        self.add_simple_action("back", |window| window.go_back());
        self.add_simple_action("forward", |window| window.go_forward());
        self.add_simple_action("home", |window| window.go_home());
        self.add_simple_action("reload", |window| window.reload());
        self.add_simple_action("preferences", |window| window.show_preferences());
        self.add_simple_action("open-in-browser", |window| window.open_current_in_browser());
        self.add_simple_action("about", |window| window.show_about());

        webview.load_uri(config::HOME_URL);
    }

    fn add_simple_action<F>(&self, name: &str, callback: F)
    where
        F: Fn(&Self) + 'static,
    {
        let action = gio::SimpleAction::new(name, None);
        action.connect_activate(clone!(
            #[weak(rename_to = window)]
            self,
            move |_action, _parameter| callback(&window)
        ));
        self.add_action(&action);
    }

    fn webview(&self) -> Option<webkit6::WebView> {
        self.imp().webview.borrow().clone()
    }

    fn go_back(&self) {
        if let Some(webview) = self.webview() {
            if webview.can_go_back() {
                webview.go_back();
            }
        }
    }

    fn go_forward(&self) {
        if let Some(webview) = self.webview() {
            if webview.can_go_forward() {
                webview.go_forward();
            }
        }
    }

    fn go_home(&self) {
        self.open_uri(config::HOME_URL);
    }

    fn reload(&self) {
        if let Some(webview) = self.webview() {
            webview.reload();
        }
    }

    fn update_navigation_state(&self) {
        if let Some(webview) = self.imp().webview.borrow().as_ref() {
            self.action_set_enabled("back", webview.can_go_back());
            self.action_set_enabled("forward", webview.can_go_forward());
        }
    }

    /// Applies the saved placement and button order to the navigation box.
    fn apply_navigation_settings(&self) {
        let settings = self.imp().settings.borrow().clone();
        let (Some(nav), Some(header), Some(overlay)) = (
            self.imp().nav.borrow().clone(),
            self.imp().header.borrow().clone(),
            self.imp().overlay.borrow().clone(),
        ) else {
            return;
        };

        // Re-append the buttons in the requested left-to-right order. Appending
        // an existing child moves it to the end of the box.
        for item in settings.order {
            let buttons = self.imp().nav_buttons.borrow();
            let Some((_, button)) = buttons.iter().find(|(candidate, _)| *candidate == item) else {
                continue;
            };
            if button.parent().is_some() {
                nav.remove(button);
            }
            nav.append(button);
        }

        // The box itself moves between the title bar and the page overlay.
        if let Some(parent) = nav.parent() {
            if let Some(current_overlay) = parent.downcast_ref::<gtk::Overlay>() {
                current_overlay.remove_overlay(&nav);
            } else {
                nav.unparent();
            }
        }

        match settings.placement {
            NavPlacement::Header => {
                nav.remove_css_class("pintrust-nav");
                nav.add_css_class("pintrust-nav-inline");
                nav.set_halign(gtk::Align::Start);
                nav.set_valign(gtk::Align::Center);
                nav.set_margin_start(0);
                nav.set_margin_end(0);
                nav.set_margin_top(0);
                nav.set_margin_bottom(0);
                header.pack_start(&nav);
            }
            placement => {
                nav.remove_css_class("pintrust-nav-inline");
                nav.add_css_class("pintrust-nav");
                nav.set_halign(match placement {
                    NavPlacement::TopLeft | NavPlacement::BottomLeft => gtk::Align::Start,
                    _ => gtk::Align::End,
                });
                nav.set_valign(match placement {
                    NavPlacement::TopLeft | NavPlacement::TopRight => gtk::Align::Start,
                    _ => gtk::Align::End,
                });
                nav.set_margin_start(18);
                nav.set_margin_end(18);
                nav.set_margin_top(18);
                nav.set_margin_bottom(18);
                overlay.add_overlay(&nav);
            }
        }
    }

    fn set_navigation_placement(&self, placement: NavPlacement) {
        {
            let mut settings = self.imp().settings.borrow_mut();
            if settings.placement == placement {
                return;
            }
            settings.placement = placement;
            settings.save();
        }
        self.apply_navigation_settings();
    }

    fn move_navigation_item(&self, index: usize, delta: isize) {
        let mut settings = self.imp().settings.borrow_mut();
        if !settings.move_item(index, delta) {
            return;
        }
        settings.save();
        drop(settings);
        self.apply_navigation_settings();
    }

    fn reset_navigation_settings(&self) {
        {
            let mut settings = self.imp().settings.borrow_mut();
            *settings = Settings::default();
            settings.save();
        }
        self.apply_navigation_settings();
    }

    fn show_preferences(&self) {
        let settings = self.imp().settings.borrow().clone();

        // Position is a radio group rather than a drop-down: radio rows only
        // change the setting when the user activates them, so nothing can be
        // written while the dialog is being built or torn down.
        let position_group = adw::PreferencesGroup::builder()
            .title("Navigation buttons")
            .description("Where the back, forward, home and reload buttons are shown.")
            .build();

        let mut checks = Vec::with_capacity(NavPlacement::ALL.len());
        let mut first: Option<gtk::CheckButton> = None;
        for placement in NavPlacement::ALL {
            let row = adw::ActionRow::builder().title(placement.label()).build();
            row.set_activatable(true);

            let check = gtk::CheckButton::new();
            if let Some(first) = &first {
                check.set_group(Some(first));
            } else {
                first = Some(check.clone());
            }
            check.set_active(placement == settings.placement);
            check.connect_toggled(clone!(
                #[weak(rename_to = window)]
                self,
                move |check| {
                    if check.is_active() {
                        window.set_navigation_placement(placement);
                    }
                }
            ));
            row.connect_activated(clone!(
                #[weak]
                check,
                move |_| check.set_active(true)
            ));

            row.add_prefix(&check);
            position_group.add(&row);
            checks.push((placement, check));
        }
        let checks = Rc::new(checks);

        let order_group = adw::PreferencesGroup::builder()
            .title("Button order")
            .description("Use the arrows to arrange the buttons from left to right.")
            .build();

        let rows: Rc<RefCell<Vec<OrderRow>>> = Rc::new(RefCell::new(Vec::new()));
        for _ in 0..NavItem::ALL.len() {
            let row = adw::ActionRow::new();
            let left = gtk::Button::from_icon_name("go-previous-symbolic");
            left.add_css_class("flat");
            left.set_tooltip_text(Some("Move left"));
            let right = gtk::Button::from_icon_name("go-next-symbolic");
            right.add_css_class("flat");
            right.set_tooltip_text(Some("Move right"));

            let suffix = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            suffix.add_css_class("linked");
            suffix.append(&left);
            suffix.append(&right);
            row.add_suffix(&suffix);

            order_group.add(&row);
            rows.borrow_mut().push(OrderRow { row, left, right });
        }
        refresh_order_rows(&rows.borrow(), &settings);

        for (index, order_row) in rows.borrow().iter().enumerate() {
            order_row.left.connect_clicked(clone!(
                #[weak(rename_to = window)]
                self,
                #[strong]
                rows,
                move |_| {
                    window.move_navigation_item(index, -1);
                    refresh_order_rows(&rows.borrow(), &window.imp().settings.borrow());
                }
            ));
            order_row.right.connect_clicked(clone!(
                #[weak(rename_to = window)]
                self,
                #[strong]
                rows,
                move |_| {
                    window.move_navigation_item(index, 1);
                    refresh_order_rows(&rows.borrow(), &window.imp().settings.borrow());
                }
            ));
        }

        let reset_row = adw::ActionRow::builder()
            .title("Reset navigation")
            .subtitle("Restore the default position and order.")
            .build();
        let reset = gtk::Button::with_label("Reset");
        reset.add_css_class("flat");
        reset.connect_clicked(clone!(
            #[weak(rename_to = window)]
            self,
            #[strong]
            rows,
            #[strong]
            checks,
            move |_| {
                window.reset_navigation_settings();
                refresh_order_rows(&rows.borrow(), &window.imp().settings.borrow());
                let default = NavPlacement::default();
                for (placement, check) in checks.iter() {
                    check.set_active(*placement == default);
                }
            }
        ));
        reset_row.add_suffix(&reset);

        let reset_group = adw::PreferencesGroup::new();
        reset_group.add(&reset_row);

        let page = adw::PreferencesPage::new();
        page.add(&position_group);
        page.add(&order_group);
        page.add(&reset_group);

        let preferences = adw::PreferencesDialog::builder()
            .title("Preferences")
            .content_width(520)
            .content_height(620)
            .follows_content_size(false)
            .build();
        preferences.add(&page);
        preferences.present(Some(self));
    }

    /// Handles URLs passed on the command line (or via the desktop file).
    pub fn open_uri(&self, uri: &str) {
        let Ok(url) = url::Url::parse(uri) else {
            webview::open_external(uri, Some(self.upcast_ref()));
            return;
        };

        if config::is_internal_url(&url) {
            self.set_page("web");
            if let Some(webview) = self.webview() {
                webview.load_uri(uri);
            }
        } else {
            webview::open_external(uri, Some(self.upcast_ref()));
        }
    }

    fn open_current_in_browser(&self) {
        let Some(webview) = self.webview() else {
            return;
        };
        let Some(uri) = webview.uri() else {
            return;
        };
        if uri.starts_with("http://") || uri.starts_with("https://") {
            webview::open_external(&uri, Some(self.upcast_ref()));
        }
    }

    fn retry(&self) {
        let uri = self
            .imp()
            .failed_uri
            .borrow_mut()
            .take()
            .unwrap_or_else(|| config::HOME_URL.to_string());

        self.set_page("web");
        if let Some(progress) = self.imp().progress.borrow().as_ref() {
            progress.set_visible(true);
            progress.set_fraction(0.0);
        }
        if let Some(webview) = self.webview() {
            webview.load_uri(&uri);
        }
    }

    fn show_error(&self, uri: &str, error: &glib::Error) {
        if !uri.is_empty() {
            self.imp().failed_uri.replace(Some(uri.to_string()));
        }

        if let Some(status_page) = self.imp().status_page.borrow().as_ref() {
            status_page.set_description(Some(&format!("{}\n\n{}", error.message(), uri)));
        }

        if let Some(progress) = self.imp().progress.borrow().as_ref() {
            progress.set_visible(false);
        }

        self.set_page("error");
    }

    fn set_page(&self, name: &str) {
        if let Some(stack) = self.imp().stack.borrow().as_ref() {
            stack.set_visible_child_name(name);
        }
    }

    fn show_about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name("PintRust")
            .application_icon(config::APP_ID)
            .developer_name("PintRust Contributors")
            .version(env!("CARGO_PKG_VERSION"))
            .website("https://www.pinterest.com/")
            .license_type(gtk::License::MitX11)
            .comments(
                "An unofficial, lightweight Pinterest desktop wrapper.\n\
                 Pinterest is a trademark of Pinterest, Inc.",
            )
            .build();
        about.present(Some(self));
    }
}

fn nav_button(item: NavItem) -> gtk::Button {
    let button = gtk::Button::from_icon_name(item.icon());
    button.set_action_name(Some(item.action()));
    button.set_tooltip_text(Some(item.tooltip()));
    button.add_css_class("flat");
    button
}

/// One reorderable row in the preferences dialog.
struct OrderRow {
    row: adw::ActionRow,
    left: gtk::Button,
    right: gtk::Button,
}

fn refresh_order_rows(rows: &[OrderRow], settings: &Settings) {
    for (index, order_row) in rows.iter().enumerate() {
        let Some(item) = settings.order.get(index) else {
            break;
        };
        order_row.row.set_title(item.label());
        order_row
            .row
            .set_subtitle(&format!("Position {}", index + 1));
        order_row.left.set_sensitive(index > 0);
        order_row.right.set_sensitive(index + 1 < settings.order.len());
    }
}
