# Cheburcheck Probe
[![Build Probe](https://github.com/LowderPlay/cheburcheck/actions/workflows/probe-build.yml/badge.svg)](https://github.com/LowderPlay/cheburcheck/actions/workflows/probe-build.yml)

Динамический сканер для Cheburcheck.
Подключается к MQTT-брокеру Cheburcheck по WebSocket, получает задания на проверку доменов, выполняет сетевые пробы со своей точки подключения и отправляет результаты обратно на сайт.

Сканер нужен для проверки «изнутри» разных сетей: например, от разных операторов, регионов или хостингов.
Он не принимает итоговое решение сам, а передает технические признаки, по которым Cheburcheck показывает результат пользователю.

## Сборка

Готовые бинарные файлы и Debian-пакеты можно скачать на [странице релизов](https://github.com/LowderPlay/cheburcheck/releases).

На Debian-based дистрибутивах можно собрать пакет через `cargo-deb`:

```shell
cargo deb --package probe -- --bin cheburprobe
```

На прочих дистрибутивах и ОС можно запустить напрямую:

```shell
cargo run --package probe --bin cheburprobe -- \
  --probe-id <ID_СКАНЕРА> \
  --probe-token <ТОКЕН_СКАНЕРА>
```

Также доступен Docker-образ, который собирается из `probe/Dockerfile`.

## Получение доступа

Для подключения сканера нужен `PROBE_ID` и `PROBE_TOKEN`.
Они должны соответствовать записи в таблице `reporters` на стороне Cheburcheck.

Чтобы получить доступ, напишите на [support@cheburcheck.ru](mailto:support@cheburcheck.ru).
В письме укажите:

- регион;
- интернет-провайдера или хостинг;
- ASN, если он известен;
- где будет запущен сканер: сервер, домашний роутер, микрокомпьютер и так далее.

## Установка как systemd-демона

Самый простой способ установки на Debian-based систему — скачать `.deb` пакет `cheburprobe` со [страницы релизов](https://github.com/LowderPlay/cheburcheck/releases).

Debian-пакет устанавливает systemd unit `cheburprobe.service` и файл конфигурации `/etc/default/cheburprobe`.
Сервис не включается автоматически: сначала нужно указать данные сканера.

1. Установите пакет:

    ```shell
    sudo apt install ./cheburprobe_*.deb
    ```

2. Настройте `/etc/default/cheburprobe`:

    ```shell
    sudo nano /etc/default/cheburprobe
    ```

    Минимальная конфигурация:

    ```shell
    PROBE_ID=1
    PROBE_TOKEN=ваш-токен
    MQTT_HOST=wss://cheburcheck.ru/mqtt
    MQTT_PORT=443
    ```

3. Запустите и включите сервис:

    ```shell
    sudo systemctl enable --now cheburprobe.service
    ```

4. Проверьте статус:

    ```shell
    systemctl status cheburprobe.service
    ```

5. Посмотрите логи:

    ```shell
    journalctl -u cheburprobe.service -f
    ```

Сервис запускается с `DynamicUser=yes`, поэтому сканеру не нужен root-доступ.

## Запуск без установки

Пример запуска из исходников:

```shell
PROBE_ID=1 \
PROBE_TOKEN=ваш-токен \
MQTT_HOST=wss://cheburcheck.ru/mqtt \
MQTT_PORT=443 \
cargo run --package probe --bin cheburprobe
```

Пример запуска через Docker:

```shell
docker run --rm \
  --cap-add NET_RAW \
  -e PROBE_ID=1 \
  -e PROBE_TOKEN=ваш-токен \
  -e MQTT_HOST=wss://cheburcheck.ru/mqtt \
  -e MQTT_PORT=443 \
  ghcr.io/lowderplay/cheburcheck-probe:latest
```

## Конфигурация

| Параметр | Описание | Значение по умолчанию |
| --- | --- | --- |
| `--mqtt-host`, `MQTT_HOST` | Адрес MQTT-брокера по WebSocket. Поддерживаются `ws://` и `wss://`. | `wss://cheburcheck.ru/mqtt` |
| `--mqtt-port`, `MQTT_PORT` | Порт MQTT-брокера. | `443` |
| `--mqtt-connection-timeout-secs`, `MQTT_CONNECTION_TIMEOUT_SECS` | Таймаут подключения к MQTT-брокеру. | `30` |
| `--probe-id`, `PROBE_ID` | ID сканера. | обязательно |
| `--probe-token`, `PROBE_TOKEN` | Секретный токен сканера. | обязательно |
| `--max-concurrent-tasks`, `MAX_CONCURRENT_TASKS` | Максимальное количество одновременных заданий. | `8` |
| `--traceroute-max-hops`, `TRACEROUTE_MAX_HOPS` | Максимальный TTL для TCP traceroute. | `5` |
| `RUST_LOG` | Уровень логирования. | `info` |

`MAX_CONCURRENT_TASKS` и `TRACEROUTE_MAX_HOPS` должны быть больше нуля. Для получения ICMP-ответов traceroute процессу требуется capability `CAP_NET_RAW`; systemd unit и Docker-образ настраивают её автоматически.

## Как работает проверка

После подключения сканер:

1. публикует retained-статус `online` в MQTT;
2. подписывается на конфигурацию динамического сканирования;
3. получает задания на проверку доменов и IP-адресов;
4. параллельно запускает SNI-проверки (для доменов), TCP traceroute до цели и контрольный TCP traceroute;
5. отправляет результат обратно в Cheburcheck.

Для каждого тестового хоста сканер открывает TCP-соединение, начинает TLS-handshake с проверяемым доменом в SNI, затем отправляет простой HTTP GET-запрос.
Проверка намеренно отключает валидацию TLS-сертификата, потому что измеряется доступность соединения, а не доверие к сертификату.

## Диагностика

Если сканер не подключается:

- проверьте `PROBE_ID` и `PROBE_TOKEN`;
- убедитесь, что `MQTT_HOST` начинается с `ws://` или `wss://`;
- проверьте доступность `MQTT_HOST:MQTT_PORT` с сервера;
- посмотрите логи через `journalctl -u cheburprobe.service -f`;
- временно установите `RUST_LOG=debug`.
