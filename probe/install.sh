#!/bin/sh
set -eu

UPDATE_API_URL=${CHEBURPROBE_UPDATE_API_BASE_URL:-https://cheburcheck.ru/api/v1/probe-updates}
WITH_LUCI=${CHEBURPROBE_WITH_LUCI:-1}
ASSUME_YES=${CHEBURPROBE_ASSUME_YES:-0}
PROBE_ID=${PROBE_ID:-}
PROBE_TOKEN=${PROBE_TOKEN:-}

log() { printf '%s\n' "cheburprobe installer: $*"; }
fail() { printf '%s\n' "cheburprobe installer: error: $*" >&2; exit 1; }
command_exists() { command -v "$1" >/dev/null 2>&1; }
has_tty() { [ -r /dev/tty ] && ( : </dev/tty ) 2>/dev/null; }

prompt() {
	message=$1
	default=$2
	if [ "$ASSUME_YES" = 1 ]; then
		return 0
	fi
	has_tty || fail "interactive terminal is required (or set CHEBURPROBE_ASSUME_YES=1)"
	while :; do
		printf '%s ' "$message" >/dev/tty
		IFS= read -r answer </dev/tty || fail "could not read the answer"
		[ -n "$answer" ] || answer=$default
		case "$answer" in
			y|Y|yes|YES|д|Д|да|ДА) return 0 ;;
			n|N|no|NO|н|Н|нет|НЕТ) return 1 ;;
			*) printf 'Введите y или n.\n' >/dev/tty ;;
		esac
	done
}

download() {
	url=$1
	destination=$2
	if command_exists curl; then
		curl --fail --location --silent --show-error "$url" --output "$destination"
	elif command_exists wget; then
		wget -q -O "$destination" "$url"
	else
		fail "curl or wget is required"
	fi
}

probe_version() {
	if [ -n "${CHEBURPROBE_VERSION:-}" ]; then
		version=${CHEBURPROBE_VERSION#v}
	else
		case "$PLATFORM" in
			debian)
				version=$(sed -n "s/.*\"name\"[[:space:]]*:[[:space:]]*\"cheburprobe_\([^\"]*\)-1_${ARCH}\\.deb\".*/\1/p" "$RELEASE_JSON" | head -n 1)
				;;
			openwrt-apk)
				version=$(sed -n "s/.*\"name\"[[:space:]]*:[[:space:]]*\"cheburprobe-\([^\"]*\)-r1_${ARCH}\\.apk\".*/\1/p" "$RELEASE_JSON" | head -n 1)
				;;
			openwrt-opkg)
				version=$(sed -n "s/.*\"name\"[[:space:]]*:[[:space:]]*\"cheburprobe_\([^\"]*\)-1_${ARCH}\\.ipk\".*/\1/p" "$RELEASE_JSON" | head -n 1)
				;;
		esac
	fi
	[ -n "$version" ] || fail "the latest release has no Cheburprobe package for $PLATFORM_NAME/$ARCH"
	case "$version" in ''|*[!0-9A-Za-z.+~-]*) fail "invalid Probe package version: $version" ;; esac
	printf '%s\n' "$version"
}

validate_openwrt_arch() {
	case "$1" in
		aarch64_generic|aarch64_cortex-a53|aarch64_cortex-a72) ;;
		*) fail "unsupported OpenWrt architecture: $1" ;;
	esac
}

detect_platform() {
	if [ -f /etc/openwrt_release ]; then
		if command_exists apk; then
			PLATFORM=openwrt-apk
			PLATFORM_NAME='OpenWrt (apk)'
			ARCH=$(apk --print-arch)
			validate_openwrt_arch "$ARCH"
		elif command_exists opkg; then
			PLATFORM=openwrt-opkg
			PLATFORM_NAME='OpenWrt (opkg)'
			ARCH=$(opkg print-architecture | awk '$2 != "all" { arch = $2 } END { print arch }')
			[ -n "$ARCH" ] || fail "opkg did not report a package architecture"
			validate_openwrt_arch "$ARCH"
		else
			fail "OpenWrt package manager apk or opkg was not found"
		fi
	elif [ -f /etc/debian_version ]; then
		PLATFORM=debian
		PLATFORM_NAME='Debian/Ubuntu'
		ARCH=$(dpkg --print-architecture)
		case "$ARCH" in amd64|arm64) ;; *) fail "unsupported Debian architecture: $ARCH" ;; esac
	else
		fail "unsupported operating system (expected Debian/Ubuntu or OpenWrt)"
	fi
}

select_packages() {
	case "$PLATFORM" in
		debian)
			PACKAGE="cheburprobe_${VERSION}-1_${ARCH}.deb"
			LUCI_PACKAGE=
			;;
		openwrt-apk)
			PACKAGE="cheburprobe-${VERSION}-r1_${ARCH}.apk"
			LUCI_PACKAGE="luci-app-cheburprobe-${VERSION}-r1.apk"
			;;
		openwrt-opkg)
			PACKAGE="cheburprobe_${VERSION}-1_${ARCH}.ipk"
			LUCI_PACKAGE="luci-app-cheburprobe_${VERSION}-1_all.ipk"
			;;
	esac
}

installed_version() {
	if command_exists cheburprobe; then
		cheburprobe --version 2>/dev/null | awk 'NR == 1 { print $2 }'
		return
	fi
	case "$PLATFORM" in
		debian)
			dpkg-query -W -f='${Version}' cheburprobe 2>/dev/null | sed 's/-[^-]*$//' || true
			;;
		openwrt-apk)
			apk info --exists cheburprobe >/dev/null 2>&1 || return 0
			apk info cheburprobe 2>/dev/null | sed -n 's/^cheburprobe-\([0-9][^-]*\)-r[0-9][0-9]*$/\1/p' | head -n 1
			;;
		openwrt-opkg)
			opkg status cheburprobe 2>/dev/null | sed -n 's/^Version: \(.*\)-[^-]*$/\1/p' | head -n 1
			;;
	esac
}

read_existing_credentials() {
	EXISTING_ID=
	EXISTING_TOKEN=
	case "$PLATFORM" in
		debian)
			if [ -f /etc/default/cheburprobe ]; then
				EXISTING_ID=$(sed -n 's/^PROBE_ID=//p' /etc/default/cheburprobe | head -n 1)
				EXISTING_TOKEN=$(sed -n 's/^PROBE_TOKEN=//p' /etc/default/cheburprobe | head -n 1)
			fi
			;;
		openwrt-*)
			EXISTING_ID=$(uci -q get cheburprobe.main.probe_id 2>/dev/null || true)
			EXISTING_TOKEN=$(uci -q get cheburprobe.main.probe_token 2>/dev/null || true)
			;;
	esac
}

read_probe_token() {
	printf 'Probe token: ' >/dev/tty
	set +e
	if command_exists stty; then
		stty -echo </dev/tty
		IFS= read -r PROBE_TOKEN </dev/tty
		read_status=$?
		stty echo </dev/tty
	else
		# BusyBox ash supports silent input as a shell built-in even when stty is absent.
		IFS= read -r -s PROBE_TOKEN </dev/tty 2>/dev/null
		read_status=$?
		if [ "$read_status" -ne 0 ]; then
			IFS= read -r PROBE_TOKEN </dev/tty
			read_status=$?
		fi
	fi
	set -e
	printf '\n' >/dev/tty
	[ "$read_status" -eq 0 ] || fail "could not read PROBE_TOKEN"
}

read_credentials() {
	CONFIGURE=0
	if { [ -n "$PROBE_ID" ] && [ -z "$PROBE_TOKEN" ]; } || { [ -z "$PROBE_ID" ] && [ -n "$PROBE_TOKEN" ]; }; then
		fail "PROBE_ID and PROBE_TOKEN must be provided together"
	fi
	if [ -n "$PROBE_ID" ]; then
		CONFIGURE=1
		return
	fi
	read_existing_credentials
	if [ -n "$EXISTING_ID" ] && [ -n "$EXISTING_TOKEN" ]; then
		printf '\nНайдены сохранённые данные авторизации для Probe ID %s.\n' "$EXISTING_ID"
		if prompt 'Использовать их? [Y/n]' y; then
			PROBE_ID=$EXISTING_ID
			PROBE_TOKEN=$EXISTING_TOKEN
			CONFIGURE=1
			return
		fi
	fi
	printf '\nБез данных авторизации пакет будет установлен, но основной сервис не будет запущен.\n'
	if [ "$ASSUME_YES" = 1 ] && ! has_tty; then
		return
	fi
	if ! prompt 'Настроить авторизацию сейчас? [Y/n]' y; then
		return
	fi
	printf 'Probe ID: ' >/dev/tty
	IFS= read -r PROBE_ID </dev/tty || fail "could not read PROBE_ID"
	read_probe_token
	[ -n "$PROBE_ID" ] && [ -n "$PROBE_TOKEN" ] || fail "ID and token must not be empty"
	case "$PROBE_ID$PROBE_TOKEN" in *'
'*) fail "ID and token must not contain newlines" ;; esac
	CONFIGURE=1
}

asset_url() {
	printf '%s/assets/%s\n' "${UPDATE_API_URL%/}" "$1"
}

download_asset() {
	asset=$1
	url=$(asset_url "$asset")
	log "downloading $asset"
	download "$url" "$WORK_DIR/$asset"
	case "$asset" in
		*.ipk)
			gzip -t "$WORK_DIR/$asset" 2>/dev/null ||
				fail "published IPK has an incompatible container; rebuild and republish the OpenWrt packages"
			;;
	esac
}
sed_replacement() { printf '%s' "$1" | sed 's/[\\&|]/\\&/g'; }

install_package() {
	download_asset "$PACKAGE"
	case "$PLATFORM" in
		debian)
			log "installing $PACKAGE"
			apt-get install -y "$WORK_DIR/$PACKAGE"
			;;
		openwrt-apk)
			set -- "$WORK_DIR/$PACKAGE"
			if [ "$WITH_LUCI" = 1 ]; then download_asset "$LUCI_PACKAGE"; set -- "$@" "$WORK_DIR/$LUCI_PACKAGE"; fi
			log "installing OpenWrt packages"
			apk --allow-untrusted add "$@"
			;;
		openwrt-opkg)
			set -- "$WORK_DIR/$PACKAGE"
			if [ "$WITH_LUCI" = 1 ]; then download_asset "$LUCI_PACKAGE"; set -- "$@" "$WORK_DIR/$LUCI_PACKAGE"; fi
			log "installing OpenWrt packages"
			opkg install "$@"
			;;
	esac
}

configure_and_start() {
	case "$PLATFORM" in
		debian)
			config=/etc/default/cheburprobe
			id=$(sed_replacement "$PROBE_ID")
			token=$(sed_replacement "$PROBE_TOKEN")
			sed -i "s|^PROBE_ID=.*|PROBE_ID=$id|; s|^PROBE_TOKEN=.*|PROBE_TOKEN=$token|" "$config"
			chmod 600 "$config"
			systemctl enable --now cheburprobe.service
			;;
		openwrt-*)
			uci set cheburprobe.main.probe_id="$PROBE_ID"
			uci set cheburprobe.main.probe_token="$PROBE_TOKEN"
			uci set cheburprobe.main.enabled='1'
			uci commit cheburprobe
			chmod 600 /etc/config/cheburprobe
			/etc/init.d/cheburprobe enable
			/etc/init.d/cheburprobe restart
			;;
	esac
}

disable_unconfigured_service() {
	case "$PLATFORM" in
		debian) systemctl disable --now cheburprobe.service >/dev/null 2>&1 || true ;;
		openwrt-*)
			uci set cheburprobe.main.enabled='0'
			uci commit cheburprobe
			/etc/init.d/cheburprobe stop >/dev/null 2>&1 || true
			/etc/init.d/cheburprobe disable >/dev/null 2>&1 || true
			;;
	esac
}

print_configuration_help() {
	printf '\nCheburprobe установлен, но не запущен.\n'
	case "$PLATFORM" in
		debian)
			printf '%s\n' 'Укажите PROBE_ID и PROBE_TOKEN в /etc/default/cheburprobe, затем выполните:'
			printf '%s\n' '  sudo systemctl enable --now cheburprobe.service'
			;;
		openwrt-*)
			if [ "$WITH_LUCI" = 1 ]; then printf '%s\n' 'Откройте Службы → Cheburprobe в LuCI, укажите ID и токен и включите сервис.'; fi
			printf '%s\n' 'Или настройте /etc/config/cheburprobe через UCI, затем выполните:'
			printf '%s\n' '  /etc/init.d/cheburprobe enable && /etc/init.d/cheburprobe start'
			;;
	esac
}

[ "$(id -u)" -eq 0 ] || fail "run this installer as root"
WORK_DIR=$(mktemp -d /tmp/cheburprobe-install.XXXXXX)
trap 'rm -rf "$WORK_DIR"' EXIT INT TERM
RELEASE_JSON=$WORK_DIR/latest.json
download "${UPDATE_API_URL%/}/releases/latest" "$RELEASE_JSON"
detect_platform
VERSION=$(probe_version)
select_packages
INSTALLED_VERSION=$(installed_version)

printf '\nCheburcheck Probe — мастер установки\n'
printf '%s\n' '------------------------------------'
printf 'Система:          %s\n' "$PLATFORM_NAME"
printf 'Архитектура:      %s\n' "$ARCH"
if [ -n "$INSTALLED_VERSION" ]; then printf 'Установлено:      v%s\n' "$INSTALLED_VERSION"; else printf '%s\n' 'Установлено:      нет'; fi
printf 'Будет установлен: v%s\n' "$VERSION"
printf 'Пакет:            %s\n' "$PACKAGE"
if [ -n "$LUCI_PACKAGE" ] && [ "$WITH_LUCI" = 1 ]; then printf 'LuCI:             %s\n' "$LUCI_PACKAGE"; fi

if [ -n "$INSTALLED_VERSION" ]; then
	if [ "$INSTALLED_VERSION" = "$VERSION" ]; then question='Последняя версия уже установлена. Переустановить её? [y/N]'; default=n; else question='Обновить Cheburprobe до указанной версии? [Y/n]'; default=y; fi
	if ! prompt "$question" "$default"; then log "update cancelled"; exit 0; fi
else
	if ! prompt 'Продолжить установку? [Y/n]' y; then log "installation cancelled"; exit 0; fi
fi

read_credentials
install_package
if [ "$CONFIGURE" = 1 ]; then
	configure_and_start
	printf '\nCheburprobe v%s настроен, запущен и добавлен в автозагрузку.\n' "$VERSION"
else
	disable_unconfigured_service
	print_configuration_help
fi
