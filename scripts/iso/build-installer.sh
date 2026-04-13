#!/bin/sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
APORTS_DIR="${APORTS_DIR:-$ROOT_DIR/.build/aports}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/out/images}"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/out/work}"
TAG="${TAG:-edge}"
REPO_MAIN="https://dl-cdn.alpinelinux.org/alpine/${TAG}/main"
REPO_COMMUNITY="https://dl-cdn.alpinelinux.org/alpine/${TAG}/community"
YUNEXAL_RELEASE_DIR="${YUNEXAL_RELEASE_DIR:-$ROOT_DIR/yunex-release}"
ENABLE_EXPERIMENTAL_ARM="${ENABLE_EXPERIMENTAL_ARM:-1}"
EXPERIMENTAL_ARM_ARCHES="${EXPERIMENTAL_ARM_ARCHES:-aarch64 armv7}"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1" >&2
        exit 1
    fi
}

if ! command -v apk >/dev/null 2>&1; then
    echo "This script must run on Alpine (apk is required)." >&2
    exit 1
fi

require_cmd git
require_cmd sh

if [ ! -f "$YUNEXAL_RELEASE_DIR/yunexal-panel" ] || [ ! -f "$YUNEXAL_RELEASE_DIR/yunexal-setup" ]; then
    echo "Build release binaries first: expected in $YUNEXAL_RELEASE_DIR" >&2
    exit 1
fi

echo "[1/5] Installing mkimage prerequisites..."
apk add --no-cache abuild alpine-conf syslinux xorriso squashfs-tools grub mtools git rsync fakeroot >/dev/null

if ! ls "$HOME/.abuild"/*.rsa >/dev/null 2>&1; then
    echo "[2/5] Generating abuild key..."
    abuild-keygen -ain >/dev/null
else
    echo "[2/5] Reusing existing abuild key."
fi

if [ ! -d "$APORTS_DIR/.git" ]; then
    echo "[3/5] Cloning aports..."
    mkdir -p "$(dirname "$APORTS_DIR")"
    git clone --depth=1 https://gitlab.alpinelinux.org/alpine/aports.git "$APORTS_DIR"
else
    echo "[3/5] Updating aports..."
    git -C "$APORTS_DIR" pull --ff-only
fi

cp "$ROOT_DIR/scripts/iso/mkimg.yunexal.sh" "$APORTS_DIR/scripts/mkimg.yunexal.sh"
cp "$ROOT_DIR/scripts/iso/genapkovl-yunexal.sh" "$APORTS_DIR/scripts/genapkovl-yunexal.sh"
chmod +x "$APORTS_DIR/scripts/genapkovl-yunexal.sh"

mkdir -p "$OUT_DIR/x86_64" "$WORK_DIR/x86_64"

echo "[4/5] Building x86_64 installer ISO (UEFI + BIOS)..."
YUNEXAL_RELEASE_DIR="$YUNEXAL_RELEASE_DIR" \
sh "$APORTS_DIR/scripts/mkimage.sh" \
    --tag "$TAG" \
    --outdir "$OUT_DIR/x86_64" \
    --workdir "$WORK_DIR/x86_64" \
    --arch x86_64 \
    --profile yunexal \
    --repository "$REPO_MAIN" \
    --repository "$REPO_COMMUNITY" \
    --checksum

if [ "$ENABLE_EXPERIMENTAL_ARM" = "1" ]; then
    echo "[5/5] Building experimental ARM images using upstream profile (rpi)..."
    for arch in $EXPERIMENTAL_ARM_ARCHES; do
        mkdir -p "$OUT_DIR/$arch" "$WORK_DIR/$arch"
        if ! sh "$APORTS_DIR/scripts/mkimage.sh" \
            --tag "$TAG" \
            --outdir "$OUT_DIR/$arch" \
            --workdir "$WORK_DIR/$arch" \
            --arch "$arch" \
            --profile rpi \
            --repository "$REPO_MAIN" \
            --repository "$REPO_COMMUNITY" \
            --checksum; then
            echo "WARN: experimental ARM build failed for $arch (continuing)." >&2
        fi
    done
else
    echo "[5/5] Skipping experimental ARM builds."
fi

echo "Done. Artifacts are in: $OUT_DIR"
