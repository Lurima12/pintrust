#!/bin/sh
# Meson invokes this from the build directory. Keeping the cargo target dir
# inside the Meson build directory makes `meson dist` and Flatpak builds clean
# and prevents stale artifacts from leaking between configurations.
set -eu

src=${1:?source directory}
builddir=${2:?build directory}
out=${3:?output binary}
profile=${4:-release}

# The GNOME SDK bundles Rust as an extension but does not put it on PATH.
if ! command -v cargo >/dev/null 2>&1; then
    if [ -x /usr/lib/sdk/rust-stable/bin/cargo ]; then
        PATH="/usr/lib/sdk/rust-stable/bin:$PATH"
        export PATH
    fi
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo-build.sh: cargo was not found in PATH" >&2
    exit 1
fi

if [ "$profile" = "debug" ]; then
    CARGO_TARGET_DIR="$builddir/cargo-target" \
        cargo build --manifest-path "$src/Cargo.toml" --locked
    binary="$builddir/cargo-target/debug/pintrust"
else
    CARGO_TARGET_DIR="$builddir/cargo-target" \
        cargo build --release --manifest-path "$src/Cargo.toml" --locked
    binary="$builddir/cargo-target/release/pintrust"
fi

cp "$binary" "$out"
