# Cheburcheck Probe

[![Build Probe](https://github.com/LowderPlay/cheburcheck/actions/workflows/probe-build.yml/badge.svg)](https://github.com/LowderPlay/cheburcheck/actions/workflows/probe-build.yml)

Cheburcheck Probe (`cheburprobe`) — динамический сетевой сканер. Он подключается к Cheburcheck, получает задания на проверку доменов, выполняет их из вашей сети и отправляет технические результаты обратно.

Сканер помогает проверять доступность из разных сетей, регионов и хостингов. Он не принимает итоговое решение сам: Cheburcheck использует собранные им признаки при формировании результата.

## Быстрый старт

### 1. Получите ID и токен

Для запуска нужны `PROBE_ID` и `PROBE_TOKEN`. Запросите их по адресу [support@cheburcheck.ru](mailto:support@cheburcheck.ru) и укажите в письме:

- регион;
- интернет-провайдера или хостинг;
- ASN, если он известен;
- устройство, на котором будет работать сканер: сервер, домашний роутер, микрокомпьютер и т. п.

Не публикуйте полученный токен и не добавляйте его в систему контроля версий.

### 2. Выберите способ установки

Готовые пакеты и бинарные файлы находятся на [странице релизов](https://github.com/LowderPlay/cheburcheck/releases), Docker-образ — в GitHub Container Registry: `ghcr.io/lowderplay/cheburcheck-probe`.

| Среда | Что использовать |
| --- | --- |
| Debian, Ubuntu и производные | `.deb` для amd64 или arm64 |
| Docker на Linux | образ для amd64 или arm64 |
| OpenWrt с `apk` (25.12+) | основной `.apk` и пакет LuCI `.apk` |
| OpenWrt с `opkg` | основной `.ipk` и пакет LuCI `.ipk` |

### 3. Автоматическая установка Debian/OpenWrt

Интерактивный мастер определяет ОС, архитектуру и пакетный менеджер, находит версию Probe среди пакетов последнего GitHub Release, показывает текущую и доступную версии, а затем запрашивает подтверждение установки или обновления. Данные авторизации можно указать сразу или настроить позднее.

На Debian/Ubuntu выполните:

```shell
curl -fsSL https://raw.githubusercontent.com/LowderPlay/cheburcheck/master/probe/install.sh | \
  sudo sh
```

На OpenWrt выполните от имени `root`:

```shell
wget -qO- https://raw.githubusercontent.com/LowderPlay/cheburcheck/master/probe/install.sh | \
  sh
```

Перед запуском можно скачать и просмотреть скрипт отдельно:

```shell
curl -fSLO https://raw.githubusercontent.com/LowderPlay/cheburcheck/master/probe/install.sh
less install.sh
sudo sh install.sh
```

Если Cheburprobe уже установлен, мастер покажет установленную и последнюю версии и спросит разрешение на обновление. Существующие настройки авторизации можно сохранить.

Для автоматизированной установки передайте `PROBE_ID`, `PROBE_TOKEN` и `CHEBURPROBE_ASSUME_YES=1`.

По умолчанию в OpenWrt также устанавливается интерфейс LuCI. Чтобы установить только сервис, передайте `CHEBURPROBE_WITH_LUCI=0`. Для установки конкретного релиза можно передать `CHEBURPROBE_VERSION`, например `CHEBURPROBE_VERSION=0.5.0`.

## Debian и Ubuntu

1. На [странице релизов](https://github.com/LowderPlay/cheburcheck/releases) скачайте файл `cheburprobe_*.deb` для архитектуры вашей системы. Проверить архитектуру можно командой `dpkg --print-architecture`.

2. Установите пакет:

   ```shell
   sudo apt install ./cheburprobe_*.deb
   ```

3. Откройте файл настроек:

   ```shell
   sudo nano /etc/default/cheburprobe
   ```

   Укажите выданные ID и токен. Адрес брокера обычно менять не требуется:

   ```shell
   PROBE_ID=1
   PROBE_TOKEN=ваш-токен
   MQTT_HOST=wss://cheburcheck.ru/mqtt
   MQTT_PORT=443
   ```

4. Запустите сервис и добавьте его в автозагрузку:

   ```shell
   sudo systemctl enable --now cheburprobe.service
   ```

5. Убедитесь, что сканер работает:

   ```shell
   systemctl status cheburprobe.service
   journalctl -u cheburprobe.service -f
   ```

Пакет устанавливает systemd-сервис `cheburprobe.service`. Сервис работает без root-доступа и получает только capability `CAP_NET_RAW`, необходимую для traceroute.

## Docker

Образ `ghcr.io/lowderplay/cheburcheck-probe:latest` доступен для Linux amd64 и arm64. Скачайте его и запустите контейнер:

```shell
docker pull ghcr.io/lowderplay/cheburcheck-probe:latest

docker run -d \
  --name cheburprobe \
  --restart unless-stopped \
  --cap-add NET_RAW \
  -e PROBE_ID=1 \
  -e PROBE_TOKEN=ваш-токен \
  ghcr.io/lowderplay/cheburcheck-probe:latest
```

Проверьте состояние и логи:

```shell
docker ps --filter name=cheburprobe
docker logs -f cheburprobe
```

Для постоянной установки удобнее хранить параметры в отдельном файле, например `cheburprobe.env`, ограничить доступ к нему и передать Docker через `--env-file cheburprobe.env`; либо использовать Docker Compose.

## OpenWrt

Готовые пакеты выпускаются для `aarch64_generic`, `aarch64_cortex-a53` и `aarch64_cortex-a72`. Со [страницы релиза](https://github.com/LowderPlay/cheburcheck/releases) нужно скачать два файла: основной пакет `cheburprobe` для архитектуры роутера и универсальный пакет интерфейса `luci-app-cheburprobe`.

Сначала определите, какой пакетный менеджер используется:

```shell
command -v apk || command -v opkg
```

### OpenWrt с `apk`

Узнайте архитектуру:

```shell
apk --print-arch
```

Например, для `aarch64_cortex-a53` выберите основной файл с `_aarch64_cortex-a53.apk` в имени. Пакеты релиза не подписаны ключом вашего OpenWrt, поэтому при локальной установке требуется `--allow-untrusted`:

```shell
apk --allow-untrusted add \
  ./cheburprobe-*_<АРХИТЕКТУРА>.apk \
  ./luci-app-cheburprobe-*.apk
```

### OpenWrt с `opkg`

Посмотрите список поддерживаемых архитектур:

```shell
opkg print-architecture
```

Выберите архитектуру процессора, например `aarch64_cortex-a53`, а не универсальную `all`, и установите оба пакета:

```shell
opkg install \
  ./cheburprobe_*_<АРХИТЕКТУРА>.ipk \
  ./luci-app-cheburprobe_*_all.ipk
```

### Настройка OpenWrt

После установки откройте **Службы → Cheburprobe** в LuCI:

1. укажите **Probe ID** и **Probe token**;
2. включите **Enable service**;
3. нажмите **Сохранить и применить**.

Настройки также доступны через UCI в `/etc/config/cheburprobe`. Файл создаётся с правами `0600`, потому что токен хранится в открытом виде. Для проверки используйте:

```shell
/etc/init.d/cheburprobe status
logread -e cheburprobe
```

## Общие настройки

Параметры можно передавать через переменные окружения или аргументы командной строки. В Debian переменные задаются в `/etc/default/cheburprobe`, в Docker — через `-e` или `--env-file`. OpenWrt настраивается через LuCI/UCI.

| Аргумент / переменная | Назначение | По умолчанию |
| --- | --- | --- |
| `--probe-id`, `PROBE_ID` | ID сканера | обязательный параметр |
| `--probe-token`, `PROBE_TOKEN` | Секретный токен | обязательный параметр |
| `--mqtt-host`, `MQTT_HOST` | URL MQTT-брокера; `ws://` или `wss://` | `wss://cheburcheck.ru/mqtt` |
| `--mqtt-port`, `MQTT_PORT` | Порт MQTT-брокера | `443` |
| `--mqtt-connection-timeout-secs`, `MQTT_CONNECTION_TIMEOUT_SECS` | Таймаут подключения, секунды | `30` |
| `--max-concurrent-tasks`, `MAX_CONCURRENT_TASKS` | Максимум одновременных заданий | `8` |
| `--traceroute-retries`, `TRACEROUTE_RETRIES` | Число одновременных TCP-попыток на каждом TTL | `3` |
| `RUST_LOG` | Уровень логирования | `info` |

`MAX_CONCURRENT_TASKS` и `TRACEROUTE_RETRIES` должны быть больше нуля.

## Автоматические обновления

Пакеты Debian и OpenWrt каждые шесть часов проверяют последний опубликованный GitHub Release. При появлении новой версии пакет обновляется, а сервис перезапускается. В OpenWrt пакет LuCI обновляется вместе с основным.

Автообновления включены по умолчанию. Отключить их в Debian можно командой:

```shell
sudo systemctl disable --now cheburprobe-update.timer
```

В OpenWrt используйте переключатель в LuCI или команды:

```shell
uci set cheburprobe.main.auto_update='0'
uci commit cheburprobe
/etc/init.d/cheburprobe-updater restart
```

Интервал проверки OpenWrt задаётся параметром `update_interval` в `/etc/config/cheburprobe`. Ручная проверка доступна независимо от настроек:

```shell
/usr/bin/cheburprobe update
```

Docker-контейнер не обновляет образ автоматически. Для обновления скачайте новый образ и пересоздайте контейнер с прежними параметрами.

## Диагностика

Если сканер не подключается:

- проверьте `PROBE_ID` и `PROBE_TOKEN`;
- убедитесь, что `MQTT_HOST` начинается с `ws://` или `wss://`;
- проверьте доступность `MQTT_HOST:MQTT_PORT` из сети сканера;
- изучите логи (`journalctl`, `docker logs` или `logread` — в зависимости от установки);
- временно задайте `RUST_LOG=debug`.

## Для разработчиков

### Запуск из исходников

Из корня репозитория:

```shell
PROBE_ID=1 \
PROBE_TOKEN=ваш-токен \
MQTT_HOST=wss://cheburcheck.ru/mqtt \
MQTT_PORT=443 \
cargo run --package probe --bin cheburprobe
```

Те же обязательные параметры можно передать аргументами:

```shell
cargo run --package probe --bin cheburprobe -- \
  --probe-id <ID_СКАНЕРА> \
  --probe-token <ТОКЕН_СКАНЕРА>
```

### Сборка пакетов

Debian-пакет собирается через `cargo-deb`:

```shell
cargo deb --package probe -- --bin cheburprobe
```

Docker-образ собирается из корня репозитория:

```shell
docker build -f probe/Dockerfile -t cheburprobe:local .
```

Для сборки OpenWrt-пакета другой архитектуры задайте `OPENWRT_ARCH`:

```shell
OPENWRT_ARCH=aarch64_cortex-a53 \
  probe/openwrt/build-apk.sh <binary> <output-dir>

OPENWRT_ARCH=aarch64_cortex-a53 \
  probe/openwrt/build-ipk.sh <binary> <output-dir>
```

### Как работает проверка

После подключения сканер:

1. публикует retained-статус `online` в MQTT;
2. подписывается на конфигурацию динамического сканирования;
3. получает задания на проверку доменов и IP-адресов;
4. параллельно запускает SNI-проверки для доменов и TCP traceroute до цели, начиная со следующего после DPI узла;
5. отправляет результат обратно в Cheburcheck.

Для каждого тестового хоста сканер открывает TCP-соединение, начинает TLS-handshake с проверяемым доменом в SNI, затем отправляет простой HTTP GET-запрос. Валидация TLS-сертификата намеренно отключена: измеряется доступность соединения, а не доверие к сертификату.
