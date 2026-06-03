#!/bin/sh

set -eu

RAW_URL="${ZIMANET_RAW_URL:-https://raw.githubusercontent.com/IceWhaleTech/EasyTier/module/release/zimanet.raw}"
RAW_SHA256="${ZIMANET_RAW_SHA256:-8e58e68ba8389a2c0208dfb051b0e9bed48a61df008d1f4b20003ea2b39ac5ae}"
RAW_PATH="/var/lib/extensions/zimanet.raw"
SERVICE_NAME="easytier-core.service"
INSTALL_SCRIPT_URL="https://raw.githubusercontent.com/IceWhaleTech/EasyTier/module/script/install-zimanet.sh"

if [ "$(id -u)" -ne 0 ]; then
    echo "Please run as root:" >&2
    echo "  curl -fsSL $INSTALL_SCRIPT_URL | sudo sh" >&2
    exit 1
fi

for tool in curl sha256sum zpkg systemctl install; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Missing required command: $tool" >&2
        exit 1
    fi
done

tmp_raw="$(mktemp /tmp/zimanet.raw.XXXXXX)"
trap 'rm -f "$tmp_raw"' EXIT

echo "Downloading zimanet raw package..."
curl -fsSL "$RAW_URL" -o "$tmp_raw"

actual_sha256="$(sha256sum "$tmp_raw" | awk '{print $1}')"
if [ "$actual_sha256" != "$RAW_SHA256" ]; then
    echo "SHA256 mismatch:" >&2
    echo "  expected: $RAW_SHA256" >&2
    echo "  actual:   $actual_sha256" >&2
    exit 1
fi

echo "Installing zimanet raw package..."
install -m 0644 "$tmp_raw" "$RAW_PATH"
zpkg install "$RAW_PATH"

echo "Starting zimanet service..."
systemctl daemon-reload
systemctl enable --now "$SERVICE_NAME"
systemctl restart "$SERVICE_NAME"

echo "zimanet installed. Current status:"
systemctl status "$SERVICE_NAME" --no-pager | sed -n '1,30p'
