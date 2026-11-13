use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use webkit6::prelude::*;

/// Applies the desktop's light/dark preference to Pinterest.
///
/// Pinterest renders the light theme on the server and ships that decision
/// inside its bootstrap payload, so `prefers-color-scheme` alone cannot switch
/// it. Scripting React (patching `JSON.parse`, observing the DOM) works but is
/// expensive on Pinterest's very large document.
///
/// This module instead re-maps Pinterest's own semantic colour tokens, which
/// every component already consumes. The palette is derived from the token
/// table Pinterest ships in its critical CSS: the greyscale ramp is swapped end
/// for end, and the translucent overlay ("wash") tokens use the dark values
/// that Pinterest itself defines next to the light ones.
///
/// Everything is one `UserStyleSheet` scoped to
/// `html[data-pintrust-theme="dark"]` plus a tiny document-start script that
/// sets that attribute, so switching themes never restyles the page in a loop.
pub struct ThemeInjector {
    manager: webkit6::UserContentManager,
    webview: glib::WeakRef<webkit6::WebView>,
    style_manager: adw::StyleManager,
    style_handler: Cell<Option<glib::SignalHandlerId>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Theme {
    Light,
    Dark,
}

impl Theme {
    fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

impl ThemeInjector {
    pub fn install(webview: &webkit6::WebView, manager: &webkit6::UserContentManager) -> Rc<Self> {
        let style_manager = adw::StyleManager::default();
        let theme = if style_manager.is_dark() {
            Theme::Dark
        } else {
            Theme::Light
        };

        // The style sheet itself is theme-independent; only the root attribute
        // it is scoped to changes.
        manager.add_style_sheet(&webkit6::UserStyleSheet::new(
            STYLE_SHEET,
            webkit6::UserContentInjectedFrames::AllFrames,
            webkit6::UserStyleLevel::User,
            &[],
            &[],
        ));

        let injector = Rc::new(Self {
            manager: manager.clone(),
            webview: webview.downgrade(),
            style_manager: style_manager.clone(),
            style_handler: Cell::new(None),
        });

        {
            let weak = Rc::downgrade(&injector);
            let handler_id = style_manager.connect_dark_notify(move |style_manager| {
                if let Some(injector) = weak.upgrade() {
                    let theme = if style_manager.is_dark() {
                        Theme::Dark
                    } else {
                        Theme::Light
                    };
                    injector.apply(theme);
                }
            });
            injector.style_handler.set(Some(handler_id));
        }

        {
            let weak = Rc::downgrade(&injector);
            webview.connect_destroy(move |_| {
                if let Some(injector) = weak.upgrade() {
                    injector.detach();
                }
            });
        }

        injector.apply(theme);
        injector
    }

    fn apply(&self, theme: Theme) {
        // Documents created from now on get the attribute before first paint.
        self.manager.remove_all_scripts();
        self.manager.add_script(&webkit6::UserScript::new(
            &theme_script(theme),
            webkit6::UserContentInjectedFrames::AllFrames,
            webkit6::UserScriptInjectionTime::Start,
            &[],
            &[],
        ));

        if let Some(webview) = self.webview.upgrade() {
            webview.set_background_color(&background_color(theme));
            webview.evaluate_javascript(
                &theme_script(theme),
                None,
                Some("pintrust:theme"),
                None::<&gio::Cancellable>,
                |result| {
                    if let Err(error) = result {
                        // Running against about:blank can fail; that is
                        // harmless because the UserScript covers real documents.
                        eprintln!("pintrust: theme switch failed: {error}");
                    }
                },
            );
        }
    }

    fn detach(&self) {
        if let Some(handler_id) = self.style_handler.take() {
            self.style_manager.disconnect(handler_id);
        }
        self.manager.remove_all_scripts();
        self.manager.remove_all_style_sheets();
    }
}

fn background_color(theme: Theme) -> gtk::gdk::RGBA {
    if theme.is_dark() {
        gtk::gdk::RGBA::new(0.059, 0.059, 0.059, 1.0)
    } else {
        gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0)
    }
}

/// Only marks the document. Pinterest's DOM is left completely untouched.
fn theme_script(theme: Theme) -> String {
    format!(
        r##"(function () {{
  var MODE = "{mode}";
  var tries = 0;
  function apply() {{
    var root = document.documentElement;
    if (!root) {{
      if (tries++ < 25) setTimeout(apply, 20);
      return;
    }}
    if (root.getAttribute("data-pintrust-theme") !== MODE) {{
      root.setAttribute("data-pintrust-theme", MODE);
    }}
  }}
  apply();
}})();"##,
        mode = theme.as_str()
    )
}

/// Pinterest's semantic colour tokens, re-mapped for a dark surface.
const STYLE_SHEET: &str = r##"html {
  color-scheme: light;
}
html[data-pintrust-theme="dark"] {
  color-scheme: dark;
  background-color: #0f0f0f !important;
  --sema-color-background-default: #0f0f0f !important;
  --sema-color-background-disabled: #232320 !important;
  --sema-color-background-elevated-opaque-default: #0f0f0f !important;
  --sema-color-background-elevated-opaque-secondary: #232320 !important;
  --sema-color-background-elevated-opaque-subtle: #1a1a18 !important;
  --sema-color-background-elevated-opaque-tertiary: #4a4a44 !important;
  --sema-color-background-elevated-scrim-opaque-gradient-from: hsl(53,29%,5%,.8) !important;
  --sema-color-background-elevated-scrim-opaque-gradient-to: hsl(53,29%,5%,.8) !important;
  --sema-color-background-elevated-wash-50-default: rgba(36,36,33,.5) !important;
  --sema-color-background-elevated-wash-60-default: rgba(36,36,33,.6) !important;
  --sema-color-background-elevated-wash-70-default: rgba(36,36,33,.7) !important;
  --sema-color-background-elevated-wash-80-default: rgba(36,36,33,.8) !important;
  --sema-color-background-elevated-wash-90-default: rgba(36,36,33,.9) !important;
  --sema-color-background-elevated-wash-95-default: rgba(36,36,33,.95) !important;
  --sema-color-background-neutral-default: #6a6a62 !important;
  --sema-color-background-neutral-weak: #1a1a18 !important;
  --sema-color-background-performance-plus-weak: #0b021d !important;
  --sema-color-background-secondary: #232320 !important;
  --sema-color-background-selected: #85857d !important;
  --sema-color-background-subtle: #1a1a18 !important;
  --sema-color-background-tertiary: #4a4a44 !important;
  --sema-color-background-wash-50-default: var( --sema-color-background-elevated-wash-50-dark ) !important;
  --sema-color-background-wash-70-default: var( --sema-color-background-elevated-wash-70-dark ) !important;
  --sema-color-border-decorative: #2d2d29 !important;
  --sema-color-border-disabled: #2d2d29 !important;
  --sema-color-border-focus-inner-default: #0f0f0f !important;
  --sema-color-border-interactive: #3a3a35 !important;
  --sema-color-border-selected: #85857d !important;
  --sema-color-hover-background-default: #0f0f0f !important;
  --sema-color-hover-background-elevated-opaque-default: #0f0f0f !important;
  --sema-color-hover-background-elevated-opaque-secondary: #232320 !important;
  --sema-color-hover-background-elevated-opaque-subtle: #1a1a18 !important;
  --sema-color-hover-background-elevated-opaque-tertiary: #4a4a44 !important;
  --sema-color-hover-background-elevated-scrim-opaque-gradient-from: hsl(60,6%,17%,.8) !important;
  --sema-color-hover-background-elevated-scrim-opaque-gradient-to: hsl(60,6%,17%,.8) !important;
  --sema-color-hover-background-elevated-wash-50-default: rgba(24,24,22,.5) !important;
  --sema-color-hover-background-elevated-wash-60-default: rgba(24,24,22,.6) !important;
  --sema-color-hover-background-elevated-wash-70-default: rgba(24,24,22,.7) !important;
  --sema-color-hover-background-elevated-wash-80-default: rgba(24,24,22,.8) !important;
  --sema-color-hover-background-elevated-wash-90-default: rgba(24,24,22,.9) !important;
  --sema-color-hover-background-elevated-wash-95-default: rgba(24,24,22,.95) !important;
  --sema-color-hover-background-neutral-default: #6a6a62 !important;
  --sema-color-hover-background-neutral-weak: #1a1a18 !important;
  --sema-color-hover-background-performance-plus-weak: #140532 !important;
  --sema-color-hover-background-secondary: #232320 !important;
  --sema-color-hover-background-selected: #85857d !important;
  --sema-color-hover-background-subtle: #1a1a18 !important;
  --sema-color-hover-background-tertiary: #4a4a44 !important;
  --sema-color-hover-background-wash-50-default: var( --sema-color-hover-background-elevated-wash-50-dark ) !important;
  --sema-color-hover-background-wash-70-default: var( --sema-color-hover-background-elevated-wash-70-dark ) !important;
  --sema-color-hover-border-decorative: #2d2d29 !important;
  --sema-color-hover-border-interactive: #3a3a35 !important;
  --sema-color-hover-border-selected: #85857d !important;
  --sema-color-hover-icon-default: #ffffff !important;
  --sema-color-hover-icon-neutral: #a3a39b !important;
  --sema-color-hover-icon-performance-plus: #7754ba !important;
  --sema-color-hover-icon-subtle: #a3a39b !important;
  --sema-color-hover-text-default: #ffffff !important;
  --sema-color-hover-text-neutral: #bcbcb4 !important;
  --sema-color-hover-text-performance-plus: #7754ba !important;
  --sema-color-hover-text-subtle: #a3a39b !important;
  --sema-color-icon-default: #ffffff !important;
  --sema-color-icon-disabled: #71716a !important;
  --sema-color-icon-neutral: #a3a39b !important;
  --sema-color-icon-performance-plus: #633bb0 !important;
  --sema-color-icon-subtle: #a3a39b !important;
  --sema-color-pressed-background-default: #0f0f0f !important;
  --sema-color-pressed-background-elevated-opaque-default: #0f0f0f !important;
  --sema-color-pressed-background-elevated-opaque-secondary: #232320 !important;
  --sema-color-pressed-background-elevated-opaque-subtle: #1a1a18 !important;
  --sema-color-pressed-background-elevated-opaque-tertiary: #4a4a44 !important;
  --sema-color-pressed-background-elevated-scrim-opaque-gradient-from: hsl(60,3%,26%,.8) !important;
  --sema-color-pressed-background-elevated-scrim-opaque-gradient-to: hsl(60,3%,26%,.8) !important;
  --sema-color-pressed-background-elevated-wash-50-default: rgba(11,11,10,.5) !important;
  --sema-color-pressed-background-elevated-wash-60-default: rgba(11,11,10,.6) !important;
  --sema-color-pressed-background-elevated-wash-70-default: rgba(11,11,10,.7) !important;
  --sema-color-pressed-background-elevated-wash-80-default: rgba(11,11,10,.8) !important;
  --sema-color-pressed-background-elevated-wash-90-default: rgba(11,11,10,.9) !important;
  --sema-color-pressed-background-elevated-wash-95-default: rgba(11,11,10,.95) !important;
  --sema-color-pressed-background-neutral-default: #6a6a62 !important;
  --sema-color-pressed-background-neutral-weak: #1a1a18 !important;
  --sema-color-pressed-background-performance-plus-weak: #200a4a !important;
  --sema-color-pressed-background-secondary: #232320 !important;
  --sema-color-pressed-background-selected: #85857d !important;
  --sema-color-pressed-background-subtle: #1a1a18 !important;
  --sema-color-pressed-background-tertiary: #4a4a44 !important;
  --sema-color-pressed-background-wash-50-default: var( --sema-color-pressed-background-elevated-wash-50-dark ) !important;
  --sema-color-pressed-background-wash-70-default: var( --sema-color-pressed-background-elevated-wash-70-dark ) !important;
  --sema-color-pressed-border-decorative: #2d2d29 !important;
  --sema-color-pressed-border-interactive: #3a3a35 !important;
  --sema-color-pressed-border-selected: #85857d !important;
  --sema-color-pressed-icon-default: #ffffff !important;
  --sema-color-pressed-icon-neutral: #a3a39b !important;
  --sema-color-pressed-icon-performance-plus: #8b6ec4 !important;
  --sema-color-pressed-icon-subtle: #a3a39b !important;
  --sema-color-pressed-text-default: #ffffff !important;
  --sema-color-pressed-text-neutral: #bcbcb4 !important;
  --sema-color-pressed-text-performance-plus: #8b6ec4 !important;
  --sema-color-pressed-text-subtle: #a3a39b !important;
  --sema-color-text-default: #ffffff !important;
  --sema-color-text-disabled: #71716a !important;
  --sema-color-text-neutral: #bcbcb4 !important;
  --sema-color-text-performance-plus: #633bb0 !important;
  --sema-color-text-subtle: #a3a39b !important;
}

html[data-pintrust-theme="dark"] body {
  background-color: #0f0f0f !important;
  color: #f2f2f2 !important;
}

/* Pinterest paints parts of its logged-out pages with inline styles that do
   not use the colour tokens at all. A user-origin !important declaration is
   the only thing that outranks an inline style. */
html[data-pintrust-theme="dark"] [style*="background: white"],
html[data-pintrust-theme="dark"] [style*="background:white"],
html[data-pintrust-theme="dark"] [style*="background-color: white"],
html[data-pintrust-theme="dark"] [style*="background-color:white"],
html[data-pintrust-theme="dark"] [style*="background: #FFFFFF"],
html[data-pintrust-theme="dark"] [style*="background:#FFFFFF"],
html[data-pintrust-theme="dark"] [style*="background: #ffffff"],
html[data-pintrust-theme="dark"] [style*="background-color: #FFFFFF"],
html[data-pintrust-theme="dark"] [style*="background-color: #ffffff"],
html[data-pintrust-theme="dark"] [style*="background-color: rgb(255, 255, 255)"],
html[data-pintrust-theme="dark"] [style*="background-color:rgba(255, 255, 255"],
html[data-pintrust-theme="dark"] [style*="background: rgba(255, 255, 255"] {
  background-color: #0f0f0f !important;
  background-image: none !important;
}

html[data-pintrust-theme="dark"] [style*="linear-gradient(180deg, #F6F6F3"],
html[data-pintrust-theme="dark"] [style*="linear-gradient(180deg, #f6f6f3"],
html[data-pintrust-theme="dark"] [style*="linear-gradient(#F6F6F3"],
html[data-pintrust-theme="dark"] [style*="linear-gradient(#f6f6f3"] {
  background: linear-gradient(180deg, #181816 0%, rgba(24, 24, 22, 0) 100%) !important;
}
"##;
