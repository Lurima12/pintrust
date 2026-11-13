use std::cell::RefCell;
use std::path::PathBuf;

use adw::prelude::*;
use url::Url;
use webkit6::prelude::*;

use crate::config;

thread_local! {
    static SESSION: RefCell<Option<webkit6::NetworkSession>> = const { RefCell::new(None) };
    static SETTINGS: RefCell<Option<webkit6::Settings>> = const { RefCell::new(None) };
}

/// One persistent network session for the whole process. WebKit does not allow
/// two sessions to share the same data/cache directory, so every window and
/// OAuth popup must reuse this object.
pub fn network_session() -> webkit6::NetworkSession {
    SESSION.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(|| {
                let data_dir = config::data_dir();
                let cache_dir = config::cache_dir();
                create_dir(&data_dir);
                create_dir(&cache_dir);

                let data = path_to_string(&data_dir);
                let cache = path_to_string(&cache_dir);

                let session = webkit6::NetworkSession::new(Some(&data), Some(&cache));

                // NetworkSession already persists cookies in its data directory.
                // Setting the storage explicitly documents where the login state
                // lives and keeps it stable across WebKit versions.
                if let Some(cookies) = session.cookie_manager() {
                    let cookie_file = data_dir.join("cookies.sqlite");
                    cookies.set_persistent_storage(
                        &path_to_string(&cookie_file),
                        webkit6::CookiePersistentStorage::Sqlite,
                    );
                }

                session
            })
            .clone()
    })
}

pub fn settings() -> webkit6::Settings {
    SETTINGS.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(|| {
                webkit6::Settings::builder()
                    .user_agent(config::user_agent())
                    .enable_smooth_scrolling(true)
                    .enable_page_cache(true)
                    .enable_2d_canvas_acceleration(true)
                    .enable_developer_extras(cfg!(debug_assertions))
                    .enable_html5_local_storage(true)
                    .enable_html5_database(true)
                    .enable_media(true)
                    .enable_media_capabilities(true)
                    .enable_mediasource(true)
                    .enable_webaudio(true)
                    .enable_webgl(true)
                    .enable_encrypted_media(true)
                    .media_playback_allows_inline(true)
                    .media_playback_requires_user_gesture(false)
                    .enable_site_specific_quirks(true)
                    .enable_back_forward_navigation_gestures(true)
                    // window.open() must still be allowed from user gestures;
                    // it is routed through the `create` signal below.
                    .javascript_can_open_windows_automatically(false)
                    .build()
            })
            .clone()
    })
}

fn create_dir(path: &PathBuf) {
    if let Err(error) = std::fs::create_dir_all(path) {
        eprintln!("pintrust: could not create {}: {error}", path.display());
    }
}

fn path_to_string(path: &PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

/// Opens a URI with the desktop's default handler.
pub fn open_external(uri: &str, parent: Option<&gtk::Window>) {
    let launcher = gtk::UriLauncher::new(uri);
    let owned_uri = uri.to_string();

    launcher.launch(parent, None::<&gio::Cancellable>, move |result| {
        if let Err(error) = result {
            eprintln!("pintrust: could not open {owned_uri}: {error}");
        }
    });
}

/// Keeps Pinterest navigation inside the window and sends ordinary third-party
/// links to the user's browser. Authentication hosts are whitelisted because
/// they are part of Pinterest's login flow.
pub fn install_navigation_policy(webview: &webkit6::WebView) {
    webview.connect_decide_policy(|webview, decision, decision_type| {
        if decision_type != webkit6::PolicyDecisionType::NavigationAction {
            return false;
        }

        let Some(navigation) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() else {
            return false;
        };
        let Some(action) = navigation.navigation_action() else {
            return false;
        };
        let Some(request) = action.request() else {
            return false;
        };
        let Some(uri) = request.uri() else {
            return false;
        };
        let Ok(url) = Url::parse(uri.as_str()) else {
            return false;
        };

        if config::is_internal_url(&url) {
            return false;
        }

        // Only divert navigations that the user explicitly initiated. Without
        // this guard, third-party iframes (ads, embeds, tracking pixels) and
        // server redirects would be sent to the system browser, which breaks
        // the page and spams the desktop.
        if action.is_redirect() || !action.is_user_gesture() {
            return false;
        }

        decision.ignore();

        let parent = webview
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        open_external(uri.as_str(), parent.as_ref());

        true
    });
}

/// Builds a themed WebView and wires navigation, popups and fullscreen.
pub fn build_webview(
    app: &adw::Application,
) -> (
    webkit6::WebView,
    std::rc::Rc<crate::theme_injector::ThemeInjector>,
) {
    let session = network_session();
    let settings = settings();
    let content_manager = webkit6::UserContentManager::new();

    let webview = webkit6::WebView::builder()
        .network_session(&session)
        .settings(&settings)
        .user_content_manager(&content_manager)
        .build();

    webview.set_background_color(&background_color());

    install_navigation_policy(&webview);
    install_popup_handler(&webview, app, &session, &settings);

    // HTML5 fullscreen from a video pin should fullscreen the native window.
    webview.connect_enter_fullscreen(|webview| {
        if let Some(window) = webview
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok())
        {
            window.fullscreen();
        }
        true
    });
    webview.connect_leave_fullscreen(|webview| {
        if let Some(window) = webview
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok())
        {
            window.unfullscreen();
        }
        true
    });

    let injector = crate::theme_injector::ThemeInjector::install(&webview, &content_manager);

    (webview, injector)
}

fn install_popup_handler(
    webview: &webkit6::WebView,
    app: &adw::Application,
    session: &webkit6::NetworkSession,
    settings: &webkit6::Settings,
) {
    use glib::clone;

    let app = app.clone();
    let session = session.clone();
    let settings = settings.clone();

    webview.connect_create(move |opener, action| {
        let target = action
            .request()
            .and_then(|request| request.uri())
            .map(|uri| uri.to_string())
            .unwrap_or_else(|| "about:blank".to_string());

        if let Ok(url) = Url::parse(&target) {
            if !config::is_internal_url(&url) {
                let parent = opener
                    .root()
                    .and_then(|root| root.downcast::<gtk::Window>().ok());
                open_external(&target, parent.as_ref());
                return None;
            }
        }

        // OAuth popups are allowed to navigate to Google/Facebook/Apple login
        // pages. They share the network session (cookies) with the opener and
        // are marked as a related view so `window.opener` keeps working.
        let popup_view = webkit6::WebView::builder()
            .network_session(&session)
            .settings(&settings)
            .related_view(opener)
            .build();
        popup_view.set_background_color(&background_color());

        install_navigation_policy(&popup_view);

        let window = adw::ApplicationWindow::builder()
            .application(&app)
            .title("Pinterest")
            .default_width(520)
            .default_height(720)
            .build();

        let header = adw::HeaderBar::new();
        let title = adw::WindowTitle::new("Pinterest", "");
        header.set_title_widget(Some(&title));

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&popup_view));
        window.set_content(Some(&toolbar_view));

        if let Some(parent) = opener
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok())
        {
            window.set_transient_for(Some(&parent));
        }

        window.set_content(Some(&toolbar_view));

        popup_view.connect_close(clone!(
            #[weak]
            window,
            move |_| window.close()
        ));
        popup_view.connect_title_notify(clone!(
            #[weak]
            window,
            move |webview| {
                if let Some(title) = webview.title() {
                    window.set_title(Some(&title));
                }
            }
        ));

        window.present();

        Some(popup_view.upcast())
    });
}

fn background_color() -> gtk::gdk::RGBA {
    if adw::StyleManager::default().is_dark() {
        gtk::gdk::RGBA::new(0.067, 0.067, 0.067, 1.0)
    } else {
        gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0)
    }
}
