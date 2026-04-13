#!/bin/sh -e

HOSTNAME="$1"
if [ -z "$HOSTNAME" ]; then
    echo "usage: $0 hostname" >&2
    exit 1
fi

if [ -z "${YUNEXAL_RELEASE_DIR:-}" ]; then
    echo "YUNEXAL_RELEASE_DIR is required" >&2
    exit 1
fi

PANEL_BIN="$YUNEXAL_RELEASE_DIR/yunexal-panel"
SETUP_BIN="$YUNEXAL_RELEASE_DIR/yunexal-setup"

if [ ! -f "$PANEL_BIN" ] || [ ! -f "$SETUP_BIN" ]; then
    echo "Missing Yunexal binaries in YUNEXAL_RELEASE_DIR=$YUNEXAL_RELEASE_DIR" >&2
    exit 1
fi

cleanup() {
    rm -rf "$tmp"
}

makefile() {
    OWNER="$1"
    PERMS="$2"
    FILENAME="$3"
    cat > "$FILENAME"
    chown "$OWNER" "$FILENAME"
    chmod "$PERMS" "$FILENAME"
}

rc_add() {
    mkdir -p "$tmp"/etc/runlevels/"$2"
    ln -sf /etc/init.d/"$1" "$tmp"/etc/runlevels/"$2"/"$1"
}

tmp="$(mktemp -d)"
trap cleanup EXIT

mkdir -p "$tmp"/etc
makefile root:root 0644 "$tmp"/etc/hostname <<EOF
$HOSTNAME
EOF

mkdir -p "$tmp"/etc/network
makefile root:root 0644 "$tmp"/etc/network/interfaces <<'EOF'
auto lo
iface lo inet loopback

auto eth0
iface eth0 inet dhcp
EOF

mkdir -p "$tmp"/etc/apk
makefile root:root 0644 "$tmp"/etc/apk/world <<'EOF'
alpine-base
alpine-conf
docker
docker-cli-compose
nginx
sudo
curl
bash
ca-certificates
util-linux
e2fsprogs
xfsprogs
btrfs-progs
gptfdisk
dosfstools
EOF

mkdir -p "$tmp"/opt/yunexal/bin
cp "$PANEL_BIN" "$tmp"/opt/yunexal/bin/yunexal-panel
cp "$SETUP_BIN" "$tmp"/opt/yunexal/bin/yunexal-setup
chmod 0755 "$tmp"/opt/yunexal/bin/yunexal-panel "$tmp"/opt/yunexal/bin/yunexal-setup

mkdir -p "$tmp"/usr/local/bin
makefile root:root 0755 "$tmp"/usr/local/bin/yunexal-panel <<'EOF'
#!/bin/sh
set -eu
mkdir -p /var/lib/yunexal/panel
cd /var/lib/yunexal/panel
exec /opt/yunexal/bin/yunexal-panel "$@"
EOF

makefile root:root 0755 "$tmp"/usr/local/bin/yunexal-setup <<'EOF'
#!/bin/sh
set -eu
mkdir -p /var/lib/yunexal/panel
cd /var/lib/yunexal/panel
exec /opt/yunexal/bin/yunexal-setup "$@"
EOF

makefile root:root 0755 "$tmp"/usr/local/bin/yunexal-install <<'EOF'
#!/bin/sh
set -eu

ACTION="prepare"
DISK=""
MODE="safe"
ROOT_SIZE_GIB="${ROOT_SIZE_GIB:-40}"
TARGET_ROOT="/mnt"
DRY_RUN=0
YES=0

usage() {
    cat <<USAGE
Usage:
  yunexal-install prepare --disk /dev/sdX [--mode safe|force] [--root-size-gib 40] [--yes] [--dry-run]
  yunexal-install finalize --disk /dev/sdX [--target-root /mnt]

Actions:
  prepare   Create GPT layout for Yunexal installer:
            p1 EFI   (512MiB, FAT32)
            p2 SYS   (ext4, install target)
            p3 DATA  (ext4 + project quota support)

  finalize  Add persistent DATA partition mount to installed system fstab.

Safety:
  --mode safe (default) blocks accidental operations on the currently running root disk.
  --mode force bypasses those checks.
USAGE
}

die() {
    echo "ERROR: $*" >&2
    exit 1
}

run() {
    if [ "$DRY_RUN" = "1" ]; then
        echo "+ $*"
        return 0
    fi
    "$@"
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "missing command: $1"
}

part_path() {
    d="$1"
    n="$2"
    case "$d" in
        *[0-9]) echo "${d}p${n}" ;;
        *) echo "${d}${n}" ;;
    esac
}

wait_for_block() {
    p="$1"
    i=0
    while [ "$i" -lt 20 ]; do
        if [ -b "$p" ]; then
            return 0
        fi
        i=$((i + 1))
        sleep 1
    done
    return 1
}

ensure_safe_disk() {
    [ "$MODE" = "safe" ] || return 0

    mounted="$(lsblk -nrpo NAME,MOUNTPOINT "$DISK" | awk '$2 != "" {print $0}')"
    if [ -n "$mounted" ]; then
        die "disk '$DISK' has mounted partitions; refusing in safe mode"
    fi

    root_src="$(findmnt -n -o SOURCE / 2>/dev/null || true)"
    if [ -n "$root_src" ] && [ "${root_src#/dev/}" != "$root_src" ]; then
        root_parent="$(lsblk -no PKNAME "$root_src" 2>/dev/null | head -n1 | tr -d '[:space:]')"
        disk_base="$(basename "$DISK")"
        root_base="$(basename "$root_src")"
        if [ "$disk_base" = "$root_base" ] || { [ -n "$root_parent" ] && [ "$disk_base" = "$root_parent" ]; }; then
            die "disk '$DISK' appears to be current root disk ($root_src); use another disk or --mode force"
        fi
    fi
}

prepare_disk() {
    [ -b "$DISK" ] || die "disk '$DISK' is not a block device"

    case "$ROOT_SIZE_GIB" in
        ''|*[!0-9]*) die "--root-size-gib must be an integer" ;;
    esac
    [ "$ROOT_SIZE_GIB" -ge 8 ] || die "--root-size-gib must be >= 8"

    require_cmd lsblk
    require_cmd findmnt
    require_cmd wipefs
    require_cmd sgdisk
    require_cmd mkfs.vfat
    require_cmd mkfs.ext4
    require_cmd partprobe

    ensure_safe_disk

    if [ "$YES" != "1" ]; then
        [ -t 0 ] || die "non-interactive mode requires --yes"
        echo "This will ERASE ALL DATA on $DISK"
        printf "Type YES to continue: "
        read -r confirm
        [ "$confirm" = "YES" ] || die "aborted"
    fi

    p1="$(part_path "$DISK" 1)"
    p2="$(part_path "$DISK" 2)"
    p3="$(part_path "$DISK" 3)"

    run wipefs -af "$DISK"
    run sgdisk --zap-all "$DISK"
    run sgdisk -n 1:1MiB:+512MiB -t 1:ef00 -c 1:YUNEXAL_EFI "$DISK"
    run sgdisk -n "2:0:+${ROOT_SIZE_GIB}GiB" -t 2:8300 -c 2:YUNEXAL_SYS "$DISK"
    run sgdisk -n 3:0:0 -t 3:8300 -c 3:YUNEXAL_DATA "$DISK"
    run partprobe "$DISK"

    if [ "$DRY_RUN" != "1" ]; then
        if command -v udevadm >/dev/null 2>&1; then
            run udevadm settle
        fi
        wait_for_block "$p1" || die "partition not found: $p1"
        wait_for_block "$p2" || die "partition not found: $p2"
        wait_for_block "$p3" || die "partition not found: $p3"
    fi

    run mkfs.vfat -F 32 "$p1"
    run mkfs.ext4 -F "$p2"
    run mkfs.ext4 -F -O project "$p3"

    echo ""
    echo "Disk prepared:"
    echo "  EFI : $p1 (FAT32)"
    echo "  SYS : $p2 (ext4)"
    echo "  DATA: $p3 (ext4 project quotas)"
    echo ""
    echo "Next steps:"
    echo "  1) Run setup-alpine and install system to $p2"
    echo "  2) Ensure target root is mounted at $TARGET_ROOT"
    echo "  3) Run: yunexal-install finalize --disk $DISK --target-root $TARGET_ROOT"
}

finalize_target() {
    [ -b "$DISK" ] || die "disk '$DISK' is not a block device"
    p3="$(part_path "$DISK" 3)"
    [ -b "$p3" ] || die "data partition not found: $p3"

    [ -d "$TARGET_ROOT/etc" ] || die "target root '$TARGET_ROOT' is not mounted"
    [ -f "$TARGET_ROOT/etc/fstab" ] || die "missing fstab in '$TARGET_ROOT/etc/fstab'"

    require_cmd blkid

    uuid="$(blkid -s UUID -o value "$p3" 2>/dev/null || true)"
    [ -n "$uuid" ] || die "failed to read UUID for $p3"

    run mkdir -p "$TARGET_ROOT/var/lib/yunexal/volumes"

    entry="UUID=$uuid /var/lib/yunexal/volumes ext4 defaults,prjquota 0 2"
    if grep -Fq "UUID=$uuid /var/lib/yunexal/volumes " "$TARGET_ROOT/etc/fstab"; then
        echo "fstab entry already present for DATA partition"
    else
        if [ "$DRY_RUN" = "1" ]; then
            echo "+ append to $TARGET_ROOT/etc/fstab: $entry"
        else
            echo "$entry" >> "$TARGET_ROOT/etc/fstab"
        fi
        echo "Added DATA mount entry to $TARGET_ROOT/etc/fstab"
    fi

    echo "Finalize complete. After first boot run: yunexal-setup"
}

if [ $# -gt 0 ]; then
    case "$1" in
        prepare|finalize)
            ACTION="$1"
            shift
            ;;
        help|-h|--help)
            usage
            exit 0
            ;;
    esac
fi

while [ $# -gt 0 ]; do
    case "$1" in
        --disk)
            [ $# -ge 2 ] || die "--disk requires value"
            DISK="$2"
            shift 2
            ;;
        --mode)
            [ $# -ge 2 ] || die "--mode requires value"
            MODE="$2"
            shift 2
            ;;
        --root-size-gib)
            [ $# -ge 2 ] || die "--root-size-gib requires value"
            ROOT_SIZE_GIB="$2"
            shift 2
            ;;
        --target-root)
            [ $# -ge 2 ] || die "--target-root requires value"
            TARGET_ROOT="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --yes|-y)
            YES=1
            shift
            ;;
        help|-h|--help)
            usage
            exit 0
            ;;
        *)
            die "unknown argument: $1"
            ;;
    esac
done

case "$MODE" in
    safe|force) ;;
    *) die "--mode must be safe or force" ;;
esac

[ -n "$DISK" ] || die "--disk is required"

case "$ACTION" in
    prepare)
        prepare_disk
        ;;
    finalize)
        finalize_target
        ;;
    *)
        die "unsupported action: $ACTION"
        ;;
esac
EOF

mkdir -p "$tmp"/var/lib/yunexal/panel
makefile root:root 0644 "$tmp"/var/lib/yunexal/panel/.env <<'EOF'
# Generated by Yunexal ISO overlay. Re-run yunexal-setup to rotate secrets.
PANEL_PORT=3000
COOKIE_SECRET=CHANGE_ME_WITH_YUNEXAL_SETUP
DATABASE_URL=sqlite:yunexal.db
EOF

mkdir -p "$tmp"/etc/init.d
makefile root:root 0755 "$tmp"/etc/init.d/yunexal-panel <<'EOF'
#!/sbin/openrc-run
name="yunexal-panel"
description="Yunexal Panel"

command="/usr/local/bin/yunexal-panel"
directory="/var/lib/yunexal/panel"
pidfile="/run/yunexal-panel.pid"
command_background="yes"
start_stop_daemon_args="--make-pidfile --pidfile ${pidfile} --stdout /var/log/yunexal-panel.log --stderr /var/log/yunexal-panel.log"

depend() {
    need net docker
    after firewall
}

start_pre() {
    checkpath --directory --owner root:root --mode 0755 /run
    checkpath --file --owner root:root --mode 0644 /var/log/yunexal-panel.log
}
EOF

mkdir -p "$tmp"/etc/motd.d
makefile root:root 0644 "$tmp"/etc/motd.d/30-yunexal <<'EOF'
Yunexal Alpine installer image is ready.
Run: yunexal-setup
Use disk helper: yunexal-install --help
EOF

rc_add devfs sysinit
rc_add dmesg sysinit
rc_add mdev sysinit
rc_add hwdrivers sysinit
rc_add modloop sysinit

rc_add hwclock boot
rc_add modules boot
rc_add sysctl boot
rc_add hostname boot
rc_add bootmisc boot
rc_add syslog boot

rc_add networking default
rc_add docker default
rc_add nginx default

rc_add mount-ro shutdown
rc_add killprocs shutdown
rc_add savecache shutdown

tar -c -C "$tmp" etc opt usr var | gzip -9n > "$HOSTNAME".apkovl.tar.gz
