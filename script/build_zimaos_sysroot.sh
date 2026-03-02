#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

HOST_TRIPLE="$(rustc -vV | awk '/^host: /{print $2}')"
TARGET_TRIPLE="${TARGET_TRIPLE:-$HOST_TRIPLE}"

WEB_CARGO_PACKAGE="${WEB_CARGO_PACKAGE:-easytier-web}"
WEB_CARGO_BIN="${WEB_CARGO_BIN:-easytier-web}"
WEB_CARGO_FEATURES="${WEB_CARGO_FEATURES:-embed}"
WEB_OUTPUT_BIN_NAME="${WEB_OUTPUT_BIN_NAME:-zimaos-easytier-web}"

CORE_CARGO_PACKAGE="${CORE_CARGO_PACKAGE:-easytier}"
CORE_CARGO_BIN="${CORE_CARGO_BIN:-easytier-core}"
CORE_CARGO_FEATURES="${CORE_CARGO_FEATURES:-}"
CORE_OUTPUT_BIN_NAME="${CORE_OUTPUT_BIN_NAME:-zimaos-easytier-core}"

SYSROOT_DIR="${SYSROOT_DIR:-$ROOT_DIR/build/sysroot}"
TEMPLATE_DIR="${TEMPLATE_DIR:-$ROOT_DIR/packaging/zimaos}"

for required_file in \
    "$TEMPLATE_DIR/zimaos-easytier-web.conf.sample" \
    "$TEMPLATE_DIR/zimaos-easytier-core.conf.sample" \
    "$TEMPLATE_DIR/zimaos-easytier-web.service" \
    "$TEMPLATE_DIR/zimaos-easytier-core.service"
do
    if [[ ! -f "$required_file" ]]; then
        echo "missing template file: $required_file" >&2
        exit 1
    fi
done

build_binary() {
    local package="$1"
    local bin="$2"
    local features="$3"

    echo "building binary..."
    echo "  package: $package"
    echo "  bin: $bin"
    echo "  target: $TARGET_TRIPLE"

    if [[ -n "$features" ]]; then
        cargo build --release --target "$TARGET_TRIPLE" --package "$package" --bin "$bin" --features "$features"
    else
        cargo build --release --target "$TARGET_TRIPLE" --package "$package" --bin "$bin"
    fi
}

build_binary "$WEB_CARGO_PACKAGE" "$WEB_CARGO_BIN" "$WEB_CARGO_FEATURES"
build_binary "$CORE_CARGO_PACKAGE" "$CORE_CARGO_BIN" "$CORE_CARGO_FEATURES"

WEB_BIN_PATH="$ROOT_DIR/target/$TARGET_TRIPLE/release/$WEB_CARGO_BIN"
CORE_BIN_PATH="$ROOT_DIR/target/$TARGET_TRIPLE/release/$CORE_CARGO_BIN"

if [[ ! -f "$WEB_BIN_PATH" ]]; then
    echo "binary not found: $WEB_BIN_PATH" >&2
    exit 1
fi

if [[ ! -f "$CORE_BIN_PATH" ]]; then
    echo "binary not found: $CORE_BIN_PATH" >&2
    exit 1
fi

echo "assembling sysroot at $SYSROOT_DIR"
rm -rf "$SYSROOT_DIR"
mkdir -p \
    "$SYSROOT_DIR/usr/bin" \
    "$SYSROOT_DIR/usr/lib/systemd/system" \
    "$SYSROOT_DIR/etc/casaos"

install -m 0755 "$WEB_BIN_PATH" "$SYSROOT_DIR/usr/bin/$WEB_OUTPUT_BIN_NAME"
install -m 0755 "$CORE_BIN_PATH" "$SYSROOT_DIR/usr/bin/$CORE_OUTPUT_BIN_NAME"
install -m 0644 "$TEMPLATE_DIR/zimaos-easytier-web.service" "$SYSROOT_DIR/usr/lib/systemd/system/zimaos-easytier-web.service"
install -m 0644 "$TEMPLATE_DIR/zimaos-easytier-core.service" "$SYSROOT_DIR/usr/lib/systemd/system/zimaos-easytier-core.service"
install -m 0644 "$TEMPLATE_DIR/zimaos-easytier-web.conf.sample" "$SYSROOT_DIR/etc/casaos/zimaos-easytier-web.conf.sample"
install -m 0644 "$TEMPLATE_DIR/zimaos-easytier-core.conf.sample" "$SYSROOT_DIR/etc/casaos/zimaos-easytier-core.conf.sample"

echo "generated files:"
find "$SYSROOT_DIR" -type f | sort

echo "checksums:"
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum \
        "$SYSROOT_DIR/usr/bin/$WEB_OUTPUT_BIN_NAME" \
        "$SYSROOT_DIR/usr/bin/$CORE_OUTPUT_BIN_NAME" \
        "$SYSROOT_DIR/usr/lib/systemd/system/zimaos-easytier-web.service" \
        "$SYSROOT_DIR/usr/lib/systemd/system/zimaos-easytier-core.service" \
        "$SYSROOT_DIR/etc/casaos/zimaos-easytier-web.conf.sample" \
        "$SYSROOT_DIR/etc/casaos/zimaos-easytier-core.conf.sample"
else
    shasum -a 256 \
        "$SYSROOT_DIR/usr/bin/$WEB_OUTPUT_BIN_NAME" \
        "$SYSROOT_DIR/usr/bin/$CORE_OUTPUT_BIN_NAME" \
        "$SYSROOT_DIR/usr/lib/systemd/system/zimaos-easytier-web.service" \
        "$SYSROOT_DIR/usr/lib/systemd/system/zimaos-easytier-core.service" \
        "$SYSROOT_DIR/etc/casaos/zimaos-easytier-web.conf.sample" \
        "$SYSROOT_DIR/etc/casaos/zimaos-easytier-core.conf.sample"
fi
