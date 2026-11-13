use std::cell::RefCell;

use adw::prelude::*;
use glib::WeakRef;

use crate::config;
use crate::window::Window;

const STYLE: &str = r#"
/* Slimmer title bar. Adwaita gives header bars a 47px minimum and 6/7px of
   vertical padding, which is a lot of chrome for a window that is mostly web
   content. */
headerbar.pintrust-header {
    min-height: 0;
}

headerbar.pintrust-header > windowhandle > box {
    padding: 2px 6px 3px 6px;
}

headerbar.pintrust-header button,
headerbar.pintrust-header menubutton > button {
    min-height: 22px;
    min-width: 22px;
    padding: 3px 7px;
}

headerbar.pintrust-header menubutton > button {
    padding: 3px;
}

headerbar.pintrust-header windowcontrols > button {
    min-height: 22px;
    min-width: 22px;
    padding: 3px;
}

/* Floating navigation pill in the bottom-right corner of the page. */
.pintrust-nav {
    padding: 5px;
    border-radius: 9999px;
    background-color: alpha(@window_bg_color, 0.92);
    box-shadow: 0 3px 12px alpha(black, 0.22),
                0 0 0 1px alpha(@window_fg_color, 0.10);
}

.pintrust-nav button {
    min-height: 24px;
    min-width: 24px;
    padding: 6px;
    border-radius: 9999px;
    background-color: transparent;
    background-image: none;
    box-shadow: none;
}

.pintrust-nav button:hover {
    background-color: alpha(@window_fg_color, 0.10);
}

.pintrust-nav button:active {
    background-color: alpha(@window_fg_color, 0.18);
}

.pintrust-nav button:disabled {
    opacity: 0.35;
}

/* Variant used when the buttons sit in the title bar again. */
.pintrust-nav-inline button {
    min-height: 24px;
    min-width: 24px;
    padding: 3px;
}

.pintrust-progress {
    border-radius: 0 0 6px 6px;
}

.pintrust-progress > trough {
    min-height: 3px;
    border: none;
    border-radius: 0 0 6px 6px;
    background-color: transparent;
}

.pintrust-progress > trough > progress {
    min-height: 3px;
    border-radius: 0 0 6px 6px;
    background-color: @accent_bg_color;
}
"#;

thread_local! {
    static MAIN_WINDOW: RefCell<Option<WeakRef<Window>>> = const { RefCell::new(None) };
}

pub fn run() -> glib::ExitCode {
    let app = adw::Application::new(Some(config::APP_ID), gio::ApplicationFlags::HANDLES_OPEN);
    app.set_resource_base_path(Some(config::RESOURCE_PATH));

    app.connect_startup(|_| {
        load_css();
        register_icons();
    });

    app.connect_activate(|app| {
        let window = main_window(app);
        window.present();
    });

    app.connect_open(|app, files, _hint| {
        let window = main_window(app);
        if let Some(uri) = files.first().map(|file| file.uri()) {
            window.open_uri(uri.as_str());
        }
        window.present();
    });

    app.set_accels_for_action("win.back", &["<Alt>Left"]);
    app.set_accels_for_action("win.forward", &["<Alt>Right"]);
    app.set_accels_for_action("win.home", &["<Alt>Home"]);
    app.set_accels_for_action("win.reload", &["<Ctrl>R", "F5"]);
    app.set_accels_for_action("win.preferences", &["<Ctrl>comma"]);
    app.set_accels_for_action("win.open-in-browser", &["<Ctrl><Shift>O"]);
    app.set_accels_for_action("win.about", &["F1"]);

    app.run()
}

fn main_window(app: &adw::Application) -> Window {
    if let Some(window) =
        MAIN_WINDOW.with(|cell| cell.borrow().as_ref().and_then(|window| window.upgrade()))
    {
        return window;
    }

    let window = Window::new(app);
    MAIN_WINDOW.with(|cell| {
        *cell.borrow_mut() = Some(window.downgrade());
    });
    window
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(STYLE);

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn register_icons() {
    gtk::Window::set_default_icon_name(config::APP_ID);

    if let Some(display) = gtk::gdk::Display::default() {
        let icon_theme = gtk::IconTheme::for_display(&display);
        icon_theme.add_resource_path(&format!("{}/icons", config::RESOURCE_PATH));
    }
}
