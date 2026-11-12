#!/usr/bin/env bash
#
# Builds a self-contained PintRust AppImage with linuxdeploy and the
# GTK plugin.
#
# Requirements on the build host:
#   - Rust toolchain (cargo)
#   - GTK4, libadwaita and WebKitGTK 6.0 development files, discoverable through
#     pkg-config (libgtk-4-dev, libadwaita-1-dev, libwebkitgtk-6.0-dev on
#     Debian/Ubuntu; gtk4-devel, libadwaita-devel, webkitgtk6.0-devel on Fedora)
#   - curl, file, find, pkg-config
#
# WebKitGTK runs the page in separate helper processes (WebKitWebProcess,
# WebKitNetworkProcess, WebKitGPUProcess). linuxdeploy only follows the main
# executable's shared-library dependencies, so this script copies the helpers
# into the AppDir, deploys the helpers' own dependencies and patches the
# compiled-in helper directory in libwebkitgtk to /var/tmp/pintrust, where
# build-aux/AppRun copies the bundled helpers at runtime.
#
# Usage:
#   ./build-appimage.sh
#
# Environment variables:
#   ARCH                  AppImage architecture (defaults to `uname -m`)
#   APPIMAGE_EXTRACT_AND_RUN=1   run the downloaded tools without FUSE
#   WEBKIT_EXEC_DIR       override the location of the WebKit helper binaries
#   TOOLS_DIR             where linuxdeploy and the GTK plugin are cached

set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

APP_ID="com.lurim412.PintRust"
APP_BINARY="pintrust"
APP_NAME="PintRust"

ARCH="${ARCH:-$(uname -m)}"
case "$ARCH" in
    x86_64|aarch64) ;;
    *)
        echo "error: unsupported architecture '$ARCH' (expected x86_64 or aarch64)" >&2
        exit 1
        ;;
esac

OUT_DIR="$ROOT/build-appimage"
APPDIR="$OUT_DIR/AppDir"
TOOLS_DIR="${TOOLS_DIR:-$ROOT/build-tools}"
ICON_SVG="$ROOT/data/icons/hicolor/scalable/apps/$APP_ID.svg"
ICON_PNG="$ROOT/data/icons/hicolor/256x256/apps/$APP_ID.png"
DESKTOP="$ROOT/data/$APP_ID.desktop"

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------

for command in cargo curl file find pkg-config python3; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "error: required command '$command' was not found" >&2
        exit 1
    fi
done

for module in gtk4 libadwaita-1 webkitgtk-6.0; do
    if ! pkg-config --exists "$module"; then
        echo "error: pkg-config cannot find '$module'." >&2
        echo "Install the GTK4, libadwaita and WebKitGTK 6.0 development packages and retry." >&2
        exit 1
    fi
done

for file in "$DESKTOP" "$ICON_SVG" "$ICON_PNG"; do
    if [ ! -f "$file" ]; then
        echo "error: required file not found: $file" >&2
        exit 1
    fi
done

# Running the linuxdeploy AppImages in containers/CI usually works without
# FUSE when this is set.
export APPIMAGE_EXTRACT_AND_RUN="${APPIMAGE_EXTRACT_AND_RUN:-1}"
export ARCH

mkdir -p "$TOOLS_DIR" "$OUT_DIR"

# ---------------------------------------------------------------------------
# Tools
# ---------------------------------------------------------------------------

fetch_tool() {
    local url="$1"
    local destination="$2"

    if [ -x "$destination" ]; then
        return
    fi

    echo "Downloading $(basename "$destination") ..."
    curl --fail --location --retry 3 --output "$destination" "$url"
    chmod +x "$destination"
}

LINUXDEPLOY="$TOOLS_DIR/linuxdeploy-$ARCH.AppImage"
GTK_PLUGIN="$TOOLS_DIR/linuxdeploy-plugin-gtk"
APPIMAGETOOL="$TOOLS_DIR/appimagetool-$ARCH.AppImage"

fetch_tool \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$ARCH.AppImage" \
    "$LINUXDEPLOY"

# The GTK plugin is distributed as a script rather than a release asset.
fetch_tool \
    "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh" \
    "$GTK_PLUGIN"

fetch_tool \
    "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage" \
    "$APPIMAGETOOL"

# linuxdeploy searches for plugins next to its own executable and in PATH.
export PATH="$TOOLS_DIR:$PATH"

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

echo "Building $APP_NAME (release) ..."
cargo build --release --locked

# ---------------------------------------------------------------------------
# Locate WebKitGTK's helper processes
# ---------------------------------------------------------------------------

find_webkit_exec_dir() {
    if [ -n "${WEBKIT_EXEC_DIR:-}" ]; then
        printf '%s\n' "$WEBKIT_EXEC_DIR"
        return 0
    fi

    local pc_libexec
    local pc_libdir
    pc_libexec="$(pkg-config --variable=libexecdir webkitgtk-6.0 2>/dev/null || true)"
    pc_libdir="$(pkg-config --variable=libdir webkitgtk-6.0 2>/dev/null || true)"

    local candidates=(
        "${pc_libexec%/}/webkitgtk-6.0"
        "${pc_libdir%/}/webkitgtk-6.0"
        "/usr/libexec/webkitgtk-6.0"
        "/usr/lib64/webkitgtk-6.0"
        "/usr/lib/webkitgtk-6.0"
        "/usr/lib/x86_64-linux-gnu/webkitgtk-6.0"
        "/usr/lib/aarch64-linux-gnu/webkitgtk-6.0"
    )

    local candidate
    for candidate in "${candidates[@]}"; do
        if [ -x "$candidate/WebKitWebProcess" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    # Last resort: ask the filesystem. Covers distributions with unusual
    # multiarch triples without hardcoding every one of them.
    local discovered
    discovered="$(find /usr/lib /usr/lib64 /usr/libexec \
        -maxdepth 5 -type f -name WebKitWebProcess -print -quit 2>/dev/null || true)"
    if [ -n "$discovered" ]; then
        dirname "$discovered"
        return 0
    fi

    return 1
}

if ! WEBKIT_HELPERS="$(find_webkit_exec_dir)"; then
    echo "error: could not find WebKitGTK's helper processes (WebKitWebProcess)." >&2
    echo "Set WEBKIT_EXEC_DIR to the directory containing WebKitWebProcess and retry." >&2
    exit 1
fi

echo "Bundling WebKit helper processes from $WEBKIT_HELPERS"

# The injected bundle is a small shared library that WebKit loads in every
# web process. It usually lives in the library directory rather than next to
# the helper executables (for example
# /usr/lib/x86_64-linux-gnu/webkitgtk-6.0/injected-bundle), so locate it
# separately.
find_webkit_injected_bundle_dir() {
    local pc_libdir
    pc_libdir="$(pkg-config --variable=libdir webkitgtk-6.0 2>/dev/null || true)"

    local candidates=(
        "${pc_libdir%/}/webkitgtk-6.0/injected-bundle"
        "/usr/lib/x86_64-linux-gnu/webkitgtk-6.0/injected-bundle"
        "/usr/lib/aarch64-linux-gnu/webkitgtk-6.0/injected-bundle"
        "/usr/lib64/webkitgtk-6.0/injected-bundle"
        "/usr/lib/webkitgtk-6.0/injected-bundle"
    )

    local candidate
    for candidate in "${candidates[@]}"; do
        if [ -f "$candidate/libwebkitgtkinjectedbundle.so" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    local discovered
    discovered="$(find /usr/lib /usr/lib64 \
        -maxdepth 5 -type f -name libwebkitgtkinjectedbundle.so -print -quit 2>/dev/null || true)"
    if [ -n "$discovered" ]; then
        dirname "$discovered"
        return 0
    fi

    return 1
}

WEBKIT_INJECTED_BUNDLE="$(find_webkit_injected_bundle_dir || true)"
if [ -n "$WEBKIT_INJECTED_BUNDLE" ]; then
    echo "Bundling WebKit injected bundle from $WEBKIT_INJECTED_BUNDLE"
fi

# ---------------------------------------------------------------------------
# Assemble AppDir
# ---------------------------------------------------------------------------

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/libexec/wk"

# appimagetool requires a top-level PNG (or XPM) matching the desktop file's
# Icon= entry to generate the root icon and .DirIcon. linuxdeploy only creates
# a top-level SVG symlink, so place the raster icon here explicitly.
install -m644 "$ICON_PNG" "$APPDIR/$APP_ID.png"

# Provide the legacy .appdata.xml name as well; appimagetool only looks for
# that spelling when it embeds upstream metadata.
mkdir -p "$APPDIR/usr/share/metainfo"
install -m644 "$ROOT/data/$APP_ID.metainfo.xml" \
    "$APPDIR/usr/share/metainfo/$APP_ID.appdata.xml"

# WebKit's subprocesses must live in the AppDir. Only the actual helpers are
# needed; distributions sometimes ship MiniBrowser and the jsc shell in the
# same directory, which would add hundreds of kilobytes for no reason.
for helper in WebKitWebProcess WebKitNetworkProcess WebKitGPUProcess; do
    if [ -x "$WEBKIT_HELPERS/$helper" ]; then
        cp -a "$WEBKIT_HELPERS/$helper" "$APPDIR/usr/libexec/wk/"
    elif [ "$helper" != "WebKitGPUProcess" ]; then
        echo "error: $WEBKIT_HELPERS/$helper is missing" >&2
        exit 1
    fi
done

if [ -n "$WEBKIT_INJECTED_BUNDLE" ]; then
    cp -a "$WEBKIT_INJECTED_BUNDLE" "$APPDIR/usr/libexec/wk/injected-bundle"
elif [ -d "$WEBKIT_HELPERS/injected-bundle" ]; then
    cp -a "$WEBKIT_HELPERS/injected-bundle" "$APPDIR/usr/libexec/wk/injected-bundle"
fi

# ---------------------------------------------------------------------------
# Run linuxdeploy
# ---------------------------------------------------------------------------

cd "$OUT_DIR"

DEPLOY_DEPS=(--deploy-deps-only "$APPDIR/usr/libexec/wk")
if [ -d "$APPDIR/usr/libexec/wk/injected-bundle" ]; then
    DEPLOY_DEPS+=(--deploy-deps-only "$APPDIR/usr/libexec/wk/injected-bundle")
fi

"$LINUXDEPLOY" \
    --appdir "$APPDIR" \
    --executable "$ROOT/target/release/$APP_BINARY" \
    --desktop-file "$DESKTOP" \
    --icon-file "$ICON_SVG" \
    --icon-file "$ICON_PNG" \
    --icon-filename "$APP_ID" \
    --custom-apprun "$ROOT/build-aux/AppRun" \
    "${DEPLOY_DEPS[@]}" \
    --plugin gtk

# ---------------------------------------------------------------------------
# Make WebKit relocatable
# ---------------------------------------------------------------------------
#
# WebKitGTK release builds (including the GNOME SDK) resolve their helper
# processes and injected bundle through paths compiled into libwebkitgtk, not
# through WEBKIT_EXEC_PATH or WEBKIT_INJECTED_BUNDLE_PATH; those environment
# variables are only read in developer-mode builds. Since an AppImage is
# mounted at a random path, the compiled-in paths would point at the host and
# the first page load would abort with "Unable to spawn a new child process"
# or a missing injected bundle.
#
# Patch both embedded absolute paths to a fixed, writable runtime directory of
# the same byte length. AppRun copies the bundled helpers and injected bundle
# into /var/tmp/pintrust. Replacing with an equal-length value avoids shifting
# anything else in the library; the extra separators are valid path syntax.
#
# /var/tmp rather than /tmp is deliberate: WebKit's bubblewrap sandbox mounts
# a fresh tmpfs on /tmp, but still read-binds the compiled-in helper path, so
# the helpers must live outside /tmp to be visible to the content process.
LIBWEBKIT="$(find "$APPDIR/usr/lib" -maxdepth 2 -type f \
    -name 'libwebkitgtk-6.0.so.4' -print -quit)"
if [ -z "$LIBWEBKIT" ]; then
    echo "error: libwebkitgtk-6.0.so.4 was not deployed into the AppDir" >&2
    exit 1
fi

python3 - "$LIBWEBKIT" "$WEBKIT_HELPERS" "${WEBKIT_INJECTED_BUNDLE:-}" <<'PY'
import os
import sys
from pathlib import Path

library = Path(sys.argv[1])
helper_dir = sys.argv[2].rstrip("/")
bundle_dir = sys.argv[3].rstrip("/")

base = b"/var/tmp/pintrust"
data = library.read_bytes()


def patch(path, data, label):
    candidates = [path, os.path.realpath(path)]
    old = None
    for candidate in candidates:
        if candidate and candidate.encode().rstrip(b"/") in data:
            old = candidate.encode().rstrip(b"/")
            break

    if old is None:
        return data, None

    if len(base) > len(old):
        sys.exit(
            f"error: the runtime directory {base.decode()!r} is longer than "
            f"WebKit's compiled-in {label} path {old.decode()!r}; the library "
            "cannot be patched in place."
        )

    new = base + b"/" * (len(old) - len(base))
    return data.replace(old, new), (old, new)


data, helper_patch = patch(helper_dir, data, "helper")
if helper_patch is None:
    sys.exit(
        "error: could not find WebKit's compiled helper directory in "
        f"{library}. Set WEBKIT_EXEC_DIR to the directory the library was "
        "built against."
    )

if bundle_dir:
    data, bundle_patch = patch(bundle_dir, data, "injected-bundle")
    if bundle_patch is None:
        print(
            "warning: could not find WebKit's compiled injected-bundle path; "
            "relying on WEBKIT_INJECTED_BUNDLE_PATH at runtime",
            file=sys.stderr,
        )
else:
    bundle_patch = None

library.write_bytes(data)

for label, result in (("helpers", helper_patch), ("injected bundle", bundle_patch)):
    if result is not None:
        old, new = result
        print(f"patched {library.name} {label}: {old.decode()} -> {new.decode()}")
PY

# ---------------------------------------------------------------------------
# Create the AppImage
# ---------------------------------------------------------------------------

"$APPIMAGETOOL" "$APPDIR" "$OUT_DIR/$APP_NAME-$ARCH.AppImage"

echo
echo "AppImage written to:"
find "$OUT_DIR" -maxdepth 1 -type f -name '*.AppImage' -printf '  %p\n'
