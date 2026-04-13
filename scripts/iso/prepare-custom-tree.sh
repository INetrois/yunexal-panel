#!/bin/sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
BASE_DIR="${BASE_DIR:-$ROOT_DIR/.build/alpine-base}"
UNPACK_DIR="${UNPACK_DIR:-$BASE_DIR/unpacked}"
CUSTOM_DIR="${CUSTOM_DIR:-$BASE_DIR/custom-tree}"
HOSTNAME="${HOSTNAME:-yunexal-installer}"
YUNEXAL_RELEASE_DIR="${YUNEXAL_RELEASE_DIR:-$ROOT_DIR/yunex-release}"

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1" >&2
        exit 1
    }
}

append_token_if_missing() {
    file_path="$1"
    token="$2"

    if [ ! -f "$file_path" ]; then
        return 0
    fi

    if grep -Fq "$token" "$file_path"; then
        return 0
    fi

    tmp_file="$file_path.tmp"
    awk -v token="$token" '
        /^[[:space:]]*APPEND[[:space:]]/ {
            print $0 token " "
            next
        }
        /^[[:space:]]*linux[[:space:]]/ {
            print $0 " " token
            next
        }
        { print }
    ' "$file_path" > "$tmp_file"
    mv "$tmp_file" "$file_path"
}

require_cmd rsync
require_cmd awk
require_cmd grep

if ! command -v fakeroot >/dev/null 2>&1; then
    echo "Missing required command: fakeroot" >&2
    echo "Install fakeroot to generate apkovl as non-root user." >&2
    exit 1
fi

if [ ! -d "$UNPACK_DIR" ]; then
    echo "Unpacked Alpine ISO directory not found: $UNPACK_DIR" >&2
    echo "Run: ./scripts/iso/fetch-base-iso.sh" >&2
    exit 1
fi

if [ ! -f "$YUNEXAL_RELEASE_DIR/yunexal-panel" ] || [ ! -f "$YUNEXAL_RELEASE_DIR/yunexal-setup" ]; then
    echo "Missing release binaries in: $YUNEXAL_RELEASE_DIR" >&2
    echo "Expected files: yunexal-panel and yunexal-setup" >&2
    exit 1
fi

mkdir -p "$CUSTOM_DIR"
rsync -a --delete "$UNPACK_DIR/" "$CUSTOM_DIR/"

tmp_overlay_dir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmp_overlay_dir"
}
trap cleanup EXIT

(
    cd "$tmp_overlay_dir"
    YUNEXAL_RELEASE_DIR="$YUNEXAL_RELEASE_DIR" fakeroot "$ROOT_DIR/scripts/iso/genapkovl-yunexal.sh" "$HOSTNAME"
)

overlay_name="$HOSTNAME.apkovl.tar.gz"
overlay_path="$tmp_overlay_dir/$overlay_name"
if [ ! -f "$overlay_path" ]; then
    echo "Failed to generate overlay: $overlay_name" >&2
    exit 1
fi

cp "$overlay_path" "$CUSTOM_DIR/$overlay_name"

apkovl_token="apkovl=$overlay_name"
append_token_if_missing "$CUSTOM_DIR/boot/syslinux/syslinux.cfg" "$apkovl_token"
append_token_if_missing "$CUSTOM_DIR/boot/grub/grub.cfg" "$apkovl_token"

cat > "$CUSTOM_DIR/YUNEXAL_BUILD_INFO.txt" <<EOF
base_unpack_dir=$UNPACK_DIR
custom_tree_dir=$CUSTOM_DIR
overlay=$overlay_name
hostname=$HOSTNAME
release_dir=$YUNEXAL_RELEASE_DIR
EOF

echo "Prepared custom tree: $CUSTOM_DIR"
echo "Injected overlay: $overlay_name"
echo "Boot params patched with: $apkovl_token"
