use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use webkit6::prelude::*;

/// Applies the desktop's light/dark preference to Pinterest.
///
/// Pinterest decides its theme while rendering on the server and embeds the
/// result in `__PWS_DATA__`. WebKitGTK's `prefers-color-scheme` support depends
/// on the XDG desktop portal (which is not always available inside AppImages),
/// so relying on the media query alone is not enough.
///
/// The injector therefore combines three layers:
///   1. a document-start `UserScript` that sets Pinterest's root attributes and
///      patches the embedded theme value before React hydrates;
///   2. a `UserStyleSheet` that sets `color-scheme` before the page paints;
///   3. a conservative CSS-variable fallback that only activates when the page
///      still renders with the opposite background colour.
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
        let initial = if style_manager.is_dark() {
            Theme::Dark
        } else {
            Theme::Light
        };

        let injector = Rc::new(Self {
            manager: manager.clone(),
            webview: webview.downgrade(),
            style_manager: style_manager.clone(),
            style_handler: Cell::new(None),
        });

        // Keep the injector alive through the StyleManager connection, but only
        // weakly reference it so closing a window can disconnect cleanly.
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

        injector.apply(initial);
        injector
    }

    fn apply(&self, theme: Theme) {
        let script = theme_script(theme);
        let style_sheet = theme_style_sheet(theme);

        self.manager.remove_all_scripts();
        self.manager.remove_all_style_sheets();

        self.manager.add_script(&webkit6::UserScript::new(
            &script,
            webkit6::UserContentInjectedFrames::AllFrames,
            webkit6::UserScriptInjectionTime::Start,
            &[],
            &[],
        ));

        self.manager.add_style_sheet(&webkit6::UserStyleSheet::new(
            style_sheet,
            webkit6::UserContentInjectedFrames::AllFrames,
            webkit6::UserStyleLevel::User,
            &[],
            &[],
        ));

        if let Some(webview) = self.webview.upgrade() {
            webview.set_background_color(&background_color(theme));
            webview.evaluate_javascript(
                &script,
                None,
                Some("pintrust:theme"),
                None::<&gio::Cancellable>,
                |result| {
                    if let Err(error) = result {
                        // Executing against about:blank can fail; that is
                        // harmless because the UserScript runs on every real
                        // document anyway.
                        eprintln!("pintrust: theme injection failed: {error}");
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
        gtk::gdk::RGBA::new(0.067, 0.067, 0.067, 1.0)
    } else {
        gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0)
    }
}

fn theme_style_sheet(theme: Theme) -> &'static str {
    // Deliberately without !important: user-origin !important declarations
    // outrank inline styles, which would prevent the live theme switch below.
    match theme {
        Theme::Light => "html { color-scheme: light; background-color: #ffffff; }",
        Theme::Dark => "html { color-scheme: dark; background-color: #111111; }",
    }
}

fn theme_script(theme: Theme) -> String {
    theme_script_template()
        .replace("__MODE__", theme.as_str())
        .replace("__DARK__", if theme.is_dark() { "true" } else { "false" })
}

fn theme_script_template() -> &'static str {
    r##"(() => {
  "use strict";
  const MODE = "__MODE__";
  const DARK = __DARK__;

  const boot = () => {
    const root = document.documentElement;
    if (!root) {
      document.addEventListener("DOMContentLoaded", boot, { once: true });
      return;
    }

    // Only touch Pinterest documents. The same user script is injected into
    // every frame of the page; rewriting Google's One Tap iframe (or another
    // embedded third-party frame) would be both useless and risky.
    const host = location.hostname.toLowerCase();
    const isPinterest =
      host === "pin.it" ||
      host.endsWith(".pin.it") ||
      /(^|\.)pinterest\.[a-z.]+$/.test(host);
    if (!isPinterest) return;

    window.__PINTRUST_THEME__ = MODE;

    // A live theme switch re-runs this script in the same document. Disconnect
    // the previous MutationObserver and cancel its timers; otherwise observers
    // from older themes keep rewriting the bootstrap payload back.
    const generation = (window.__PINTRUST_THEME_GEN__ || 0) + 1;
    window.__PINTRUST_THEME_GEN__ = generation;
    if (window.__PINTRUST_OBSERVER__) {
      window.__PINTRUST_OBSERVER__.disconnect();
      window.__PINTRUST_OBSERVER__ = null;
    }
    for (const timer of window.__PINTRUST_TIMERS__ || []) clearTimeout(timer);
    window.__PINTRUST_TIMERS__ = [];
    const later = (fn, delay) => {
      const id = setTimeout(() => {
        if (window.__PINTRUST_THEME_GEN__ === generation) fn();
      }, delay);
      window.__PINTRUST_TIMERS__.push(id);
    };

    const DARK_VARS = {
      "--sema-color-background-default": "#111111",
      "--sema-color-background-elevation": "#1b1b1b",
      "--sema-color-background-secondary": "#232323",
      "--sema-color-text-default": "#f2f2f2",
      "--sema-color-text-subtle": "#b0b0b0",
      "--sema-color-icon-default": "#f2f2f2",
      "--sema-color-border-default": "#333333",
      "--sema-color-border-interactive": "#4a4a4a"
    };
    const LIGHT_VARS = {
      "--sema-color-background-default": "#ffffff",
      "--sema-color-background-elevation": "#ffffff",
      "--sema-color-background-secondary": "#f6f6f3",
      "--sema-color-text-default": "#111111",
      "--sema-color-text-subtle": "#5f5f5f",
      "--sema-color-icon-default": "#111111",
      "--sema-color-border-default": "#e0e0e0",
      "--sema-color-border-interactive": "#767676"
    };

    const setAttributes = () => {
      root.setAttribute("data-color-mode", MODE);
      root.setAttribute("data-theme", MODE);
      root.setAttribute("data-pintrust-theme", MODE);
      root.classList.toggle("pintrust-dark", DARK);
      root.classList.toggle("pintrust-light", !DARK);
      root.style.setProperty("color-scheme", MODE, "important");

      const colorScheme = document.querySelector('meta[name="color-scheme"]');
      if (colorScheme) colorScheme.setAttribute("content", MODE);
      const themeColor = document.querySelector('meta[name="theme-color"]');
      if (themeColor) {
        themeColor.setAttribute("content", DARK ? "#111111" : "#ffffff");
      }
    };

    // Pinterest embeds {"theme":"light"} in __PWS_DATA__ and parses it during
    // hydration. Patching JSON.parse before that happens makes the client-side
    // render pick the DarkMode component variants.
    if (!window.__PINTRUST_PATCH_JSON__) {
      const nativeParse = JSON.parse;
      window.__PINTRUST_PATCH_JSON__ = (value, mode) => {
        if (!value || typeof value !== "object") return;
        if (!Array.isArray(value) && Object.prototype.hasOwnProperty.call(value, "theme")) {
          if (value.theme === "light" || value.theme === "dark") {
            value.theme = mode;
          }
        }
        for (const key of Object.keys(value)) {
          const child = value[key];
          if (child && typeof child === "object") {
            window.__PINTRUST_PATCH_JSON__(child, mode);
          }
        }
      };
      JSON.parse = function (text, reviver) {
        const parsed = nativeParse.call(this, text, reviver);
        try {
          window.__PINTRUST_PATCH_JSON__(parsed, window.__PINTRUST_THEME__);
        } catch (_) {}
        return parsed;
      };
    }

    // Also rewrite the inline JSON payload itself, which is how Pinterest's
    // server-rendered data reaches the app in newer builds.
    const patchBootstrap = () => {
      document
        .querySelectorAll("script#__PWS_DATA__, script#__PWS_INITIAL_PROPS__")
        .forEach((script) => {
          if (script.dataset.pintrustTheme === MODE) return;
          try {
            script.textContent = script.textContent.replace(
              /"theme"\s*:\s*"(?:light|dark)"/g,
              '"theme":"' + MODE + '"'
            );
            script.dataset.pintrustTheme = MODE;
          } catch (_) {}
        });
    };

    const clearFallback = () => {
      root.classList.remove("pintrust-force-dark", "pintrust-force-light");
      for (const key of Object.keys(DARK_VARS)) {
        root.style.removeProperty(key);
      }
      root.style.removeProperty("background-color");
      if (document.body) {
        document.body.style.removeProperty("background-color");
      }
    };

    // Last resort for builds where Pinterest ignores both the media query and
    // the injected theme: sample the real background colour and only then
    // override the semantic colour tokens.
    const detectAndFix = () => {
      clearFallback();

      const elements = [
        document.querySelector("#__PWS_ROOT__ > div"),
        document.querySelector("#__PWS_ROOT__"),
        document.body,
        root
      ];

      let luminance = null;
      for (const element of elements) {
        if (!element) continue;
        const color = getComputedStyle(element).backgroundColor;
        const match = /rgba?\(([\d.]+),\s*([\d.]+),\s*([\d.]+)(?:,\s*([\d.]+))?\)/.exec(color);
        if (!match) continue;
        const alpha = match[4] === undefined ? 1 : parseFloat(match[4]);
        if (alpha < 0.1) continue;
        luminance =
          (0.2126 * parseFloat(match[1]) +
            0.7152 * parseFloat(match[2]) +
            0.0722 * parseFloat(match[3])) /
          255;
        break;
      }

      if (luminance === null) return;
      const pageIsDark = luminance < 0.5;
      if (pageIsDark === DARK) return;

      const variables = DARK ? DARK_VARS : LIGHT_VARS;
      for (const key of Object.keys(variables)) {
        root.style.setProperty(key, variables[key], "important");
      }
      root.classList.add(DARK ? "pintrust-force-dark" : "pintrust-force-light");
      root.style.setProperty(
        "background-color",
        DARK ? "#111111" : "#ffffff",
        "important"
      );
      if (document.body) {
        document.body.style.setProperty(
          "background-color",
          DARK ? "#111111" : "#ffffff",
          "important"
        );
      }
    };

    clearFallback();
    setAttributes();
    patchBootstrap();

    const observer = new MutationObserver(() => patchBootstrap());
    window.__PINTRUST_OBSERVER__ = observer;
    observer.observe(root, { childList: true, subtree: true });

    document.addEventListener("DOMContentLoaded", () => {
      if (window.__PINTRUST_THEME_GEN__ !== generation) return;
      setAttributes();
      patchBootstrap();
      later(detectAndFix, 250);
    }, { once: true });
    window.addEventListener("load", () => {
      if (window.__PINTRUST_THEME_GEN__ !== generation) return;
      observer.disconnect();
      later(detectAndFix, 400);
    }, { once: true });
    later(detectAndFix, 1200);
  };

  boot();
})();
"##
}
