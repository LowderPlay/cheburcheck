<script lang="ts">
import KbArticle from "$lib/components/kb/KbArticle.svelte";
import KbCode from "$lib/components/kb/KbCode.svelte";
import KbHeading from "$lib/components/kb/KbHeading.svelte";
import KbNote from "$lib/components/kb/KbNote.svelte";
</script>

<svelte:head>
	<title>Как запустить свой сканер - Cheburcheck</title>
	<meta
		name="description"
		content="Как получить токен и запустить динамический сканер Cheburcheck в своей сети"
	>
	<meta property="og:title" content="Как запустить свой сканер">
	<meta
		property="og:description"
		content="Установка и запуск динамического сканера Cheburcheck"
	>
	<meta property="og:url" content="https://cheburcheck.ru/kb/probe">
</svelte:head>

<KbArticle>
	<h1>Как запустить свой сканер</h1>

	<p>
		Сканер Cheburcheck выполняет динамические проверки из вашей сети и передаёт
		результаты на сайт. Так можно добавить наблюдения из своего региона и от
		своего провайдера. Подробнее о проверках — в
		<a href="/kb/probing">статье о динамическом сканировании</a>.
	</p>

	<KbHeading id="получите-доступ" title="Получите доступ">
		Получите доступ
	</KbHeading>
	<p>
		Для запуска нужны ID сканера (<code>PROBE_ID</code>) и токен (<code
			>PROBE_TOKEN</code
		>). Чтобы получить их,
		<a href="https://forms.gle/nZqBfXC65GebUKWB9">заполните форму</a>. Не
		публикуйте токен и не добавляйте его в репозиторий.
	</p>

	<KbHeading id="установите-сканер" title="Установите сканер">
		Установите сканер
	</KbHeading>
	<p>
		На Debian, Ubuntu и OpenWrt можно воспользоваться интерактивным мастером. Он
		предложит установить подходящий пакет и ввести полученные ID и токен. На
		Debian или Ubuntu выполните:
	</p>
	<KbCode copyable
		>curl -fsSL https://cheburcheck.ru/install-probe.sh | sudo sh</KbCode
	>
	<p>На OpenWrt выполните от имени <code>root</code>:</p>
	<KbCode copyable
		>wget -qO- https://cheburcheck.ru/install-probe.sh | sh</KbCode
	>
	<p>
		На OpenWrt мастер также установит интерфейс LuCI. После установки сканер
		можно настроить в разделе <b>Службы → Cheburprobe</b>. Готовые пакеты для
		ручной установки доступны на
		<a href="https://github.com/LowderPlay/cheburcheck/releases"
			>странице релизов</a
		>.
	</p>

	<KbHeading id="docker" title="Запуск в Docker">Запуск в Docker</KbHeading>
	<p>
		Для Linux на amd64 или arm64 доступен образ
		<code>ghcr.io/lowderplay/cheburcheck-probe:latest</code>. Создайте файл
		<code>cheburprobe.env</code>
		с выданными значениями и ограничьте доступ к нему:
	</p>
	<!-- biome-ignore format: code block -->
	<KbCode copyable>
PROBE_ID=ваш-id
PROBE_TOKEN=ваш-токен
	</KbCode>
	<KbCode copyable>chmod 600 cheburprobe.env</KbCode>
	<p>Запустите контейнер:</p>
	<!-- biome-ignore format: code block -->
	<KbCode copyable>
docker run -d \
  --name cheburprobe \
  --restart unless-stopped \
  --cap-add NET_RAW \
  --env-file cheburprobe.env \
  ghcr.io/lowderplay/cheburcheck-probe:latest
	</KbCode>

	<KbHeading id="проверьте-работу" title="Проверьте работу">
		Проверьте работу
	</KbHeading>
	<p>После запуска посмотрите состояние сервиса и его журнал:</p>
	<ul>
		<li>
			Debian/Ubuntu: <code>systemctl status cheburprobe.service</code> и
			<code>journalctl -u cheburprobe.service -f</code>.
		</li>
		<li>
			OpenWrt: <code>/etc/init.d/cheburprobe status</code> и
			<code>logread -e cheburprobe</code>.
		</li>
		<li>Docker: <code>docker logs -f cheburprobe</code>.</li>
	</ul>
	<KbNote variant="info">
		Если сканер не подключается, проверьте ID и токен, а также доступность
		<code>wss://cheburcheck.ru/mqtt</code>
		из сети сканера. Дополнительные параметры и способы ручной установки описаны
		в
		<a
			href="https://github.com/LowderPlay/cheburcheck/blob/master/probe/README.md"
			>документации сканера</a
		>.
	</KbNote>
</KbArticle>
