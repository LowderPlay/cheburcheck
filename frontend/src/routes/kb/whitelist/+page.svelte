<script lang="ts">
import { createQuery } from "@tanstack/svelte-query";
import { BarChart } from "layerchart";
import { fetchWhitelistHistogram } from "$lib/api/whitelist";
import KbArticle from "$lib/components/kb/KbArticle.svelte";
import KbCaption from "$lib/components/kb/KbCaption.svelte";
import KbChartFrame from "$lib/components/kb/KbChartFrame.svelte";
import KbCode from "$lib/components/kb/KbCode.svelte";
import KbFileLink from "$lib/components/kb/KbFileLink.svelte";
import KbHeading from "$lib/components/kb/KbHeading.svelte";
import KbNote from "$lib/components/kb/KbNote.svelte";

const histogram30kQuery = createQuery(() => ({
	queryKey: ["whitelist", "histogram", 30_000, "filtered"],
	queryFn: () => fetchWhitelistHistogram(30_000, true),
}));

const histogram1mQuery = createQuery(() => ({
	queryKey: ["whitelist", "histogram", 1_000_000],
	queryFn: () => fetchWhitelistHistogram(1_000_000),
}));

const histogramError = $derived(
	histogram30kQuery.isError || histogram1mQuery.isError,
);

const chartSeries = [
	{
		key: "count",
		label: "Домены",
		value: "count",
		color: "#ef4444",
	},
];

const chartPadding = {
	top: 8,
	right: 8,
	bottom: 76,
	left: 32,
};

const chartProps = {
	xAxis: {
		tickSpacing: 25,
		tickLabelProps: {
			rotate: -35,
			textAnchor: "end",
			verticalAnchor: "middle",
			dx: -6,
			dy: 10,
		},
	},
} as const;
</script>

<svelte:head>
	<title>Белые списки - Cheburcheck</title>
	<meta name="description" content="Анализ данных из белых списков блокировок">
	<meta property="og:title" content="Белые списки">
	<meta
		property="og:description"
		content="Анализ данных из белых списков блокировок"
	>
	<meta property="og:url" content="https://cheburcheck.ru/kb/whitelist">
</svelte:head>

<KbArticle>
	<h1>Белые списки доменов</h1>
	<!-- biome-ignore format: link punctuation -->
	<KbNote>
		Не путать с
		<a href="https://habr.com/ru/news/1000784/">
			белыми списками <i>мобильного</i> интернета</a>!
	</KbNote>

	<p>
		Российские операторы связи начали применять новый тип блокировок CDN, при
		котором загрузка контента обрывается после передачи примерно 16–20 килобайт
		данных, из‑за чего большинство сайтов на Cloudflare и других зарубежных
		платформах становятся практически неработоспособными для пользователей в
		России. Одновременно сохраняются и развиваются белые списки с популярными и
		«социально значимыми» доменами, которые старательно выводятся из‑под
		подобных ограничений.
	</p>
	<p>
		Суть схемы в том, что соединение технически устанавливается, первые
		килобайты HTML, CSS или скриптов передаются, но затем трафик режется или
		соединение принудительно сбрасывается на уровне российских операторов,
		обычно после примерно 16 килобайт (10–14 пакетов в зависимости от
		протокола). Этого объёма достаточно, чтобы страница начала загружаться и
		создавалась иллюзия доступности, но ключевые части сайта (основной HTML,
		JS‑бандлы, стили, API‑запросы) не проходят, и сервис фактически перестаёт
		работать.
	</p>
	<!-- biome-ignore format: code block -->
	<KbCode>
$ curl -k https://cheburcheck.ru/100MB.bin -o/dev/null -r 0-65536 --resolve cheburcheck.ru:443:5.78.7.195 --max-time 5
  % Total    % Received % Xferd  Average Speed   Time    Time     Time  Current
                                 Dload  Upload   Total   Spent    Left  Speed
 24 65537   24 16101    0     0   3220      0  0:00:20  0:00:04  0:00:16     0
curl: (28) Operation timed out after 5000 milliseconds with 16101 out of 65537 bytes received
	</KbCode>
	<p>
		Определение домена и выбор, «разрешить или задушить» запрос, в текущей схеме
		делается на связке DPI‑фильтров. По умолчанию, все HTTP(s) запросы на адреса
		из подсетей хостеров блокируются. Однако, запросы к некоторым доменам из тех
		же подсетей проходят нормально &mdash; они обнаруживаются по открытому
		расширению TLS SNI, либо по заголовку Host (в случае с plain http).
	</p>
	<p>
		Проверить нахождение домена в белом списке можно с помощью простого
		HTTPS-запроса с подменой SNI на адрес из заблокированных диапазонов:
	</p>
	<!-- biome-ignore format: code block -->
	<KbCode>
$ curl -k https://ok.ru/100MB.bin -o/dev/null -r 0-65536 --resolve ok.ru:443:5.78.7.195
  % Total    % Received % Xferd  Average Speed   Time    Time     Time  Current
                                 Dload  Upload   Total   Spent    Left  Speed
100 65537  100 65537    0     0  55226      0  0:00:01  0:00:01 --:--:-- 55258
	</KbCode>
	<p>
		Также, стоит отметить, что домены белого списка добавляются с маской по
		поддоменам:
	</p>
	<!-- biome-ignore format: code block -->
	<KbCode>
$ curl -k https://ok.ru/100MB.bin -o/dev/null -r 0-65536 --resolve ok.ru:443:5.78.7.195
  % Total    % Received % Xferd  Average Speed   Time    Time     Time  Current
                                 Dload  Upload   Total   Spent    Left  Speed
100 65537  100 65537    0     0  55226      0  0:00:01  0:00:01 --:--:-- 55258
	</KbCode>

	<KbHeading
		id="автоматическое-сканирование"
		title="Автоматическое сканирование"
	>
		Автоматическое сканирование
	</KbHeading>
	<!-- biome-ignore format: link punctuation -->
	<p>
		Используя методы, указанные выше, мы разработали инструмент для
		автоматического сканирования и анализа блокировок
		<b>
			<a href="https://github.com/LowderPlay/cheburcheck/tree/master/reporter">
				Cheburcheck Reporter</a></b>.
	</p>

	<!-- biome-ignore format: link punctuation -->
	<p>
		Исходя из анализа 1,000,000 доменов из рейтинга
		<a href="https://tranco-list.eu/list/2NPQ9">
			Tranco list от 26 ноября 2025</a>, в белом списке содержится около 1000 доменов.
	</p>

	<i>
		* - Мы не включаем в это число домены из зоны .co.uk, так как по какой-то
		причине, они все находятся в белом списке.
	</i>

	<KbCaption>
		Гистограмма количества доменов относительно их положения в рейтинге
		(топ-30k, без .co.uk)
	</KbCaption>
	{#if histogramError}
		<KbNote>Не удалось загрузить данные гистограммы.</KbNote>
	{/if}
	<KbChartFrame>
		<BarChart
			data={histogram30kQuery.data ?? []}
			x="label"
			y="count"
			series={chartSeries}
			padding={chartPadding}
			props={chartProps}
		/>
	</KbChartFrame>

	<KbCaption>
		Гистограмма количества доменов относительно их положения в рейтинге
		(топ-1kk, включая .co.uk)
	</KbCaption>
	<KbChartFrame>
		<BarChart
			data={histogram1mQuery.data ?? []}
			x="label"
			y="count"
			series={chartSeries}
			padding={chartPadding}
			props={chartProps}
		/>
	</KbChartFrame>

	<KbHeading id="скачать-списки" title="Скачать списки">
		Скачать списки
	</KbHeading>
	<p>Мы публикуем результаты наших сканирований в виде CSV-файлов:</p>
	<KbFileLink href="/whitelist/full.csv">Полный список (CSV)</KbFileLink>
	<KbFileLink href="/whitelist/domains.csv">Только домены (CSV)</KbFileLink>
</KbArticle>
