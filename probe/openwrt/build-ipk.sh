#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
	echo "usage: $0 <cheburprobe-binary> <output-directory>" >&2
	exit 2
fi

BINARY=$1
OUTPUT_DIR=$2
ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT INT TERM

mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR=$(CDPATH= cd -- "$OUTPUT_DIR" && pwd)
VERSION=$(sed -n '/^name = "probe"$/,/^\[/{s/^version = "\([^"]*\)"$/\1/p;}' "$ROOT_DIR/probe/Cargo.toml")
ARCH=${OPENWRT_ARCH:-aarch64_generic}

test -n "$VERSION"
test -f "$BINARY"

build_ipk() {
	package_name=$1
	package_arch=$2
	package_root=$3
	control_extra=$4
	archive_name="${package_name}_${VERSION}-1_${package_arch}.ipk"
	size_kib=$(du -sk "$package_root/data" | cut -f1)

	cat > "$package_root/control/control" <<EOF
Package: ${package_name}
Version: ${VERSION}-1
Architecture: ${package_arch}
Maintainer: Lowder <me@lowderplay.dev>
License: BSD-3-Clause
Section: net
Priority: optional
Installed-Size: ${size_kib}
${control_extra}
EOF
	printf '2.0\n' > "$package_root/debian-binary"
	(cd "$package_root/control" && tar --sort=name --owner=0 --group=0 --numeric-owner -czf ../control.tar.gz .)
	(cd "$package_root/data" && tar --sort=name --owner=0 --group=0 --numeric-owner -czf ../data.tar.gz .)
	(cd "$package_root" && ar r "$OUTPUT_DIR/$archive_name" debian-binary control.tar.gz data.tar.gz)
	echo "$OUTPUT_DIR/$archive_name"
}

PROBE_ROOT="$WORK_DIR/cheburprobe"
mkdir -p "$PROBE_ROOT/control" "$PROBE_ROOT/data/usr/bin" \
	"$PROBE_ROOT/data/etc/init.d" "$PROBE_ROOT/data/etc/config"
install -m 0755 "$BINARY" "$PROBE_ROOT/data/usr/bin/cheburprobe"
install -m 0755 "$ROOT_DIR/probe/openwrt/cheburprobe.init" "$PROBE_ROOT/data/etc/init.d/cheburprobe"
install -m 0600 "$ROOT_DIR/probe/openwrt/cheburprobe.config" "$PROBE_ROOT/data/etc/config/cheburprobe"
install -m 0755 "$ROOT_DIR/probe/openwrt/cheburprobe.postinst" "$PROBE_ROOT/control/postinst"
printf '/etc/config/cheburprobe\n' > "$PROBE_ROOT/control/conffiles"
build_ipk cheburprobe "$ARCH" "$PROBE_ROOT" "Description: Dynamic network probe daemon for Cheburcheck
 Connects to Cheburcheck and performs network measurements from OpenWrt."

LUCI_ROOT="$WORK_DIR/luci-app-cheburprobe"
mkdir -p "$LUCI_ROOT/control" "$LUCI_ROOT/data/usr/share/luci/menu.d" \
	"$LUCI_ROOT/data/usr/share/rpcd/acl.d" \
	"$LUCI_ROOT/data/www/luci-static/resources/view/cheburprobe"
install -m 0644 "$ROOT_DIR/probe/openwrt/luci/menu.json" \
	"$LUCI_ROOT/data/usr/share/luci/menu.d/luci-app-cheburprobe.json"
install -m 0644 "$ROOT_DIR/probe/openwrt/luci/acl.json" \
	"$LUCI_ROOT/data/usr/share/rpcd/acl.d/luci-app-cheburprobe.json"
install -m 0644 "$ROOT_DIR/probe/openwrt/luci/config.js" \
	"$LUCI_ROOT/data/www/luci-static/resources/view/cheburprobe/config.js"
build_ipk luci-app-cheburprobe all "$LUCI_ROOT" "Depends: cheburprobe, luci-base
Description: LuCI configuration interface for Cheburprobe"
