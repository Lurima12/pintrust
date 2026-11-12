use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::clone;
use webkit6::prelude::*;

use crate::config;
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
        let header = adw::HeaderBar::new();

        let back = header_button("go-previous-symbolic", "win.back", "Back (Alt+Left)");
        let forward = header_button("go-next-symbolic", "win.forward", "Forward (Alt+Right)");
        let nav = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        nav.add_css_class("linked");
        nav.append(&back);
        nav.append(&forward);
        header.pack_start(&nav);

        let home = header_button("go-home-symbolic", "win.home", "Home (Alt+Home)");
        header.pack_start(&home);

        let reload = header_button("view-refresh-symbolic", "win.reload", "Reload (Ctrl+R)");
        header.pack_end(&reload);

        let menu = gio::Menu::new();
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

fn header_button(icon: &str, action: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_action_name(Some(action));
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("flat");
    button
}
