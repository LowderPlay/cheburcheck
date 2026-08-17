#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
	echo "usage: $0 <cheburprobe-binary> <output-directory>" >&2
	exit 2
fi

BINARY=$1
OUTPUT_DIR=$2
APK=${APK:-apk}
ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT INT TERM

mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR=$(CDPATH= cd -- "$OUTPUT_DIR" && pwd)
VERSION=$(sed -n '/^name = "probe"$/,/^\[/{s/^version = "\([^"]*\)"$/\1/p;}' "$ROOT_DIR/probe/Cargo.toml")
ARCH=${OPENWRT_ARCH:-aarch64_generic}
PACKAGE_VERSION="${VERSION}-r1"

test -n "$VERSION"
test -f "$BINARY"
"$APK" --version | grep -q '^apk-tools 3\.' || {
	echo "apk-tools 3.x is required to build OpenWrt APK packages" >&2
	exit 1
}

add_openwrt_metadata() {
	package_name=$1
	package_root=$2
	conffile=${3:-}
	metadata_dir="$package_root/lib/apk/packages"

	mkdir -p "$metadata_dir"
	(cd "$package_root" && find . \( -type f -o -type l \) | sed 's|^\./|/|' | sort) \
		> "$metadata_dir/$package_name.list"

	if [ -n "$conffile" ]; then
		printf '%s\n' "$conffile" > "$metadata_dir/$package_name.conffiles"
		checksum=$(sha256sum "$package_root$conffile" | cut -d ' ' -f1)
		printf '%s %s\n' "$conffile" "$checksum" \
			> "$metadata_dir/$package_name.conffiles_static"
	fi
}

build_apk() {
	package_name=$1
	package_arch=$2
	package_root=$3
	description=$4
	depends=$5
	post_install=${6:-}
	filename_arch=
	if [ "${PACKAGE_FILE_ARCH_SUFFIX:-0}" = 1 ] && [ "$package_arch" != noarch ]; then
		filename_arch="_${package_arch}"
	fi
	output="$OUTPUT_DIR/${package_name}-${PACKAGE_VERSION}${filename_arch}.apk"

	set -- \
		mkpkg \
		--info "name:$package_name" \
		--info "version:$PACKAGE_VERSION" \
		--info "tags:openwrt:section=net" \
		--info "description:$description" \
		--info "arch:$package_arch" \
		--info "license:BSD-3-Clause" \
		--info "origin:cheburcheck/probe" \
		--info "url:https://github.com/LowderPlay/cheburcheck" \
		--info "maintainer:Lowder <me@lowderplay.dev>" \
		--info "depends:$depends"
	if [ -n "$post_install" ]; then
		set -- "$@" --script "post-install:$post_install"
	fi
	SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-0} "$APK" "$@" \
		--no-xattrs --files "$package_root" --output "$output"
	echo "$output"
}

PROBE_ROOT="$WORK_DIR/cheburprobe"
mkdir -p "$PROBE_ROOT/usr/bin" "$PROBE_ROOT/etc/init.d" "$PROBE_ROOT/etc/config"
install -m 0755 "$BINARY" "$PROBE_ROOT/usr/bin/cheburprobe"
install -m 0755 "$ROOT_DIR/probe/openwrt/cheburprobe.init" "$PROBE_ROOT/etc/init.d/cheburprobe"
install -m 0600 "$ROOT_DIR/probe/openwrt/cheburprobe.config" "$PROBE_ROOT/etc/config/cheburprobe"
add_openwrt_metadata cheburprobe "$PROBE_ROOT" /etc/config/cheburprobe
build_apk cheburprobe "$ARCH" "$PROBE_ROOT" \
	"Dynamic network probe daemon for Cheburcheck" "" \
	"$ROOT_DIR/probe/openwrt/cheburprobe.postinst"

LUCI_ROOT="$WORK_DIR/luci-app-cheburprobe"
mkdir -p "$LUCI_ROOT/usr/share/luci/menu.d" "$LUCI_ROOT/usr/share/rpcd/acl.d" \
	"$LUCI_ROOT/www/luci-static/resources/view/cheburprobe"
install -m 0644 "$ROOT_DIR/probe/openwrt/luci/menu.json" \
	"$LUCI_ROOT/usr/share/luci/menu.d/luci-app-cheburprobe.json"
install -m 0644 "$ROOT_DIR/probe/openwrt/luci/acl.json" \
	"$LUCI_ROOT/usr/share/rpcd/acl.d/luci-app-cheburprobe.json"
install -m 0644 "$ROOT_DIR/probe/openwrt/luci/config.js" \
	"$LUCI_ROOT/www/luci-static/resources/view/cheburprobe/config.js"
add_openwrt_metadata luci-app-cheburprobe "$LUCI_ROOT"
build_apk luci-app-cheburprobe noarch "$LUCI_ROOT" \
	"LuCI configuration interface for Cheburprobe" "cheburprobe luci-base"
