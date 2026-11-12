# PintRust

A lightweight, native Linux desktop wrapper for [Pinterest](https://www.pinterest.com),
built with Rust, GTK4, libadwaita and WebKitGTK 6.0. The name is a portmanteau
of Pinterest and Rust.

> PintRust is an unofficial client. It is not affiliated with or
> endorsed by Pinterest, Inc. Pinterest is a trademark of Pinterest, Inc.

## Features

- Native GTK4 / libadwaita window with a compact header bar: Back, Forward,
  Home, Reload, main menu.
- Loading progress indicator overlaid at the top of the page.
- Persistent cookies, local storage, IndexedDB and cache in
  `$XDG_DATA_HOME/com.lurim412.PintRust` and
  `$XDG_CACHE_HOME/com.lurim412.PintRust`.
- Keeps Pinterest and its regional domains (`pinterest.de`, `pin.it`, ...)
  inside the application.
- Keeps Google, Apple and Microsoft login pages in the app so OAuth sign-in can
  complete. Facebook is only kept in-app for authentication paths; other
  Facebook links go to the system browser.
- Opens all other links with the desktop's default browser via
  `gtk::UriLauncher`.
- Synchronises the system light/dark preference with Pinterest immediately,
  without reloading the page.
- HTML5 fullscreen (video pins) fullscreens the native window.
- Hardware-accelerated rendering, smooth scrolling, media playback and
  WebGL enabled.
- Uses a current Safari user agent, which avoids Pinterest's feature gating for
  "unsupported" browsers.

## Requirements

### Runtime

- GTK 4.16 or newer
- libadwaita 1.6 or newer
- WebKitGTK 6.0 (WebKitGTK 2.46 or newer)
- GLib / GIO 2.80 or newer

### Build

- Rust 1.85 or newer (edition 2024)
- Cargo
- Meson 0.63+ and Ninja
- Development headers for GTK4, libadwaita and WebKitGTK 6.0
- `pkg-config`

Package names by distribution:

| Distribution | Packages |
| --- | --- |
| Debian / Ubuntu | `build-essential cargo meson ninja-build libgtk-4-dev libadwaita-1-dev libwebkitgtk-6.0-dev` |
| Fedora | `cargo meson ninja-build gtk4-devel libadwaita-devel webkitgtk6.0-devel` |
| Arch Linux | `rust meson ninja gtk4 libadwaita webkitgtk-6.0` |

On distributions that ship WebKitGTK 6.0 only in a newer release, build inside
the GNOME Flatpak SDK instead (see below).

## Build and run

### With Cargo

```sh
cargo run --release
```

This is the quickest way to iterate. The application uses the same persistent
data directory as an installed build.

### With Meson

```sh
meson setup build --buildtype=release -Dprofile=release
meson compile -C build
sudo meson install -C build
```

`meson install` installs the binary, desktop entry, AppStream metadata and the
scalable icon. The `profile` option accepts `release` (default) or `debug`.

To build without installing:

```sh
meson setup build --buildtype=debug -Dprofile=debug
meson compile -C build
./build/pintrust
```

### Flatpak

The manifest targets the current GNOME runtime (`org.gnome.Platform//51`):

```sh
flatpak remote-add --if-not-exists flathub \
    https://flathub.org/repo/flathub.flatpakrepo

flatpak install flathub org.gnome.Platform//51 org.gnome.Sdk//51

flatpak-builder --user --install --force-clean \
    build-dir flatpak/com.lurim412.PintRust.json

flatpak run com.lurim412.PintRust
```

The GNOME SDK includes the Rust toolchain under `/usr/lib/sdk/rust-stable`;
`build-aux/cargo-build.sh` adds it to `PATH` automatically. The manifest allows
network access during the build so Cargo can download crates; use a vendored
source directory if you need fully offline, reproducible builds.

### AppImage

```sh
chmod +x build-appimage.sh
./build-appimage.sh
```

The script downloads `linuxdeploy`, its GTK plugin and `appimagetool` into
`build-tools/`, builds the release binary, bundles the GTK/WebKit libraries,
and writes the AppImage to `build-appimage/`.

`APPIMAGE_EXTRACT_AND_RUN=1` is enabled by default so the packaging tools also
work on systems without FUSE.

#### How WebKit is packaged in the AppImage

WebKitGTK renders pages in separate processes:

- `WebKitWebProcess` (page/JavaScript)
- `WebKitNetworkProcess` (networking)
- `WebKitGPUProcess` (accelerated compositing)
- the `injected-bundle` resource directory

linuxdeploy only inspects the main executable's shared libraries, so those
helper executables would be missing from a plain AppDir. `build-appimage.sh`
therefore:

1. locates the helper directory (`/usr/libexec/webkitgtk-6.0`,
   `/usr/lib/<triple>/webkitgtk-6.0`, or the path reported by `pkg-config`) and
   the separate injected-bundle directory
   (`<libdir>/webkitgtk-6.0/injected-bundle`),
2. copies the helper binaries and `injected-bundle` to
   `AppDir/usr/libexec/wk`,
3. asks linuxdeploy to deploy the helpers' own shared-library dependencies,
4. rewrites both helper and injected-bundle directory strings compiled into
   `libwebkitgtk-6.0.so.4` to the fixed path `/var/tmp/pintrust`,
5. installs `build-aux/AppRun`, which creates `/var/tmp/pintrust` (mode `0700`,
   owned by the current user) and copies the bundled helpers and injected
   bundle into it before launching the application.

Step 4 matters because release WebKitGTK builds ignore `WEBKIT_EXEC_PATH`;
the variable is only honoured by developer-mode builds. A missing injected
bundle makes the web process abort as well, so both paths are patched. Without
patching, the library would exec `/usr/libexec/webkitgtk-6.0/...` from the
host, which either does not exist or belongs to a different WebKit version, and
page rendering would abort with `Unable to spawn a new child process`.

Helpers are copied rather than symlinked because WebKit's own bubblewrap
sandbox mounts a fresh tmpfs on `/tmp` and does not expose the AppImage mount;
a symlink pointing back into the AppDir is therefore unresolvable for the
sandboxed content process. The helpers are small (a few KiB each), so copying
them is effectively free. `/var/tmp` is used instead of `/tmp` for the same
reason: it remains visible inside WebKit's sandbox.

The replacement strings have exactly the same byte length as the originals, so
nothing else in the library shifts; the extra `/` characters are valid path
separators. `AppRun` is wrapped by linuxdeploy so the GTK plugin's hooks still
run first.

## Configuration

| Environment variable | Purpose |
| --- | --- |
| `PINTRUST_USER_AGENT` | Override the bundled Safari user agent. Useful if Pinterest changes its browser sniffing. |
| `WEBKIT_EXEC_DIR` | AppImage build only: override the directory containing `WebKitWebProcess`. |

### Data locations

| Data | Path |
| --- | --- |
| Cookies, local storage, IndexedDB | `$XDG_DATA_HOME/com.lurim412.PintRust` |
| Web cache | `$XDG_CACHE_HOME/com.lurim412.PintRust` |

With the default environment this is `~/.local/share/com.lurim412.PintRust`
and `~/.cache/com.lurim412.PintRust`. Inside Flatpak it is
`~/.var/app/com.lurim412.PintRust/...`.

## How light/dark synchronisation works

Pinterest chooses its theme on the server and embeds the result in
`__PWS_DATA__`. WebKitGTK's `prefers-color-scheme` support depends on the XDG
desktop portal, which is not always available (notably in AppImages). The
wrapper therefore uses three layers:

1. **Document-start user script**: sets `data-color-mode`, `data-theme`,
   `data-pintrust-theme`, the `color-scheme` style and a marker class on
   `<html>` before React hydrates. It also patches `JSON.parse` and rewrites
   inline `__PWS_DATA__` / `__PWS_INITIAL_PROPS__` payloads so client-side
   rendering selects the correct theme components.
2. **User style sheet**: sets `color-scheme` and a background colour before the
   first paint, avoiding a white flash when switching to dark.
3. **Conservative fallback**: after load, samples the rendered background
   luminance. Only if it still disagrees with the system theme does it override
   Pinterest's semantic colour variables (`--sema-color-*`).

`adw::StyleManager::dark` is connected to the injector, so toggling the system
theme takes effect immediately in every open window and OAuth popup. The style
sheet deliberately avoids `!important`; user-origin `!important` rules would
otherwise outrank the injected inline styles during a live theme switch.

## Project layout

```
pintrust/
├── .gitignore
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
├── build.rs                       # embeds the SVG icon as a GResource
├── meson.build
├── meson_options.txt
├── build-aux/
│   ├── AppRun                     # AppImage entry point (helper staging)
│   └── cargo-build.sh             # Meson -> Cargo bridge
├── build-appimage.sh
├── data/
│   ├── com.lurim412.PintRust.desktop
│   ├── com.lurim412.PintRust.metainfo.xml
│   ├── resources.gresource.xml
│   └── icons/hicolor/
│       ├── scalable/apps/com.lurim412.PintRust.svg
│       └── 256x256/apps/com.lurim412.PintRust.png
├── flatpak/
│   └── com.lurim412.PintRust.json
└── src/
    ├── main.rs                    # resource registration and entry point
    ├── application.rs             # adw::Application, CSS, icon theme, accels
    ├── config.rs                  # app ID, URLs, domain policy, XDG paths
    ├── theme_injector.rs          # live light/dark synchronisation
    ├── webview.rs                 # session, settings, policies, popups
    └── window.rs                  # main window, header bar, error page
```

## Navigation policy

| URL | Destination |
| --- | --- |
| `pinterest.com` and all regional `pinterest.*` domains, `pin.it` | In-app |
| `accounts.google.com`, `appleid.apple.com`, `login.microsoftonline.com` | In-app |
| Facebook login/dialog/oauth/checkpoint/recover paths | In-app |
| Any other `http(s)` link | System browser |
| `mailto:`, `tel:`, custom schemes | System handler |
| `about:`, `blob:`, `data:` | In-app |

OAuth popups reuse the process-wide `NetworkSession` and are created with
`related_view(opener)`, so `window.opener`, cookies and the login flow behave
like in a normal browser.

## Troubleshooting

### The window is blank or shows an error page inside an AppImage

Check that the helper processes are present and that the compiled-in path was
patched:

```sh
./PintRust-x86_64.AppImage --appimage-extract
ls squashfs-root/usr/libexec/wk
strings -a squashfs-root/usr/lib/libwebkitgtk-6.0.so.4 | grep -m1 '/var/tmp/pintrust'
```

Run the AppImage from a terminal to see WebKit's stderr output. If
`/var/tmp/pintrust` already exists but is owned by another user, remove it and
start the AppImage again; `AppRun` refuses to reuse a directory it cannot
verify. If the filesystem containing `/var/tmp` is mounted `noexec`, WebKit
cannot launch its helpers; mount it with `exec` or run the Flatpak build
instead.

### WebKit's sandbox fails on an unusual distribution

The bundled WebKit uses bubblewrap for its sandbox. If `bwrap` is unavailable
or incompatible, run the AppImage with
`WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1`. This disables a security boundary;
prefer fixing the host's bubblewrap installation.

### Pinterest shows a "browser not supported" page

Override the user agent:

```sh
PINTRUST_USER_AGENT="Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36" pintrust
```

### Login does not persist

Make sure `$XDG_DATA_HOME` is writable. In Flatpak, verify that no cleanup tool
removed `~/.var/app/com.lurim412.PintRust`. Do not run two PintRust
instances with different `XDG_DATA_HOME` values against the same account if you
expect shared cookies.

## License

MIT. See [LICENSE](LICENSE).

Pinterest's website, content and trademarks belong to Pinterest, Inc. Use the
wrapper in accordance with Pinterest's terms of service.
