<script lang="ts">
import KbArticle from "$lib/components/kb/KbArticle.svelte";
import KbHeading from "$lib/components/kb/KbHeading.svelte";
</script>

<svelte:head>
	<title>FAQ - Cheburcheck</title>
	<meta name="description" content="Ответы на частые вопросы">
	<meta property="og:title" content="FAQ">
	<meta property="og:description" content="Ответы на частые вопросы">
	<meta property="og:url" content="https://cheburcheck.ru/kb/faq">
</svelte:head>

<KbArticle>
	<h1>Ответы на частые вопросы</h1>
	<p>
		На этой странице собраны ответы на часто задаваемые вопросы. Если вас
		интересует что-то ещё - пишите нам на почту
		<a href="mailto:support@cheburcheck.ru"> support@cheburcheck.ru </a>
		.
	</p>

	<KbHeading id="что-это-за-сайт">Что это за сайт?</KbHeading>
	<p>
		Этот сайт - инструмент, который позволяет проверить, находится ли сайт,
		домен, IP-адрес в
		<a href="#откуда-вы-берете-эти-списки">списках</a>
		, блокируемых Роскомнадзором на территории России.
	</p>

	<KbHeading id="откуда-вы-берете-эти-списки">
		Откуда вы берете эти списки?
	</KbHeading>
	<p>
		Для поиска сайтов мы используем списки IP-подсетей популярных
		CDN-провайдеров, которые могут блокироваться Роскомнадзором, а также домены
		из самого реестра Роскомнадзора.
	</p>
	<p>
		Подсети, которые мы используем, публикуют сами провайдеры, либо собираются
		из принадлежащих им автономных сетей. Мы используем
		<a href="https://github.com/123jjck/cdn-ip-ranges">этот репозиторий</a>
		, в котором автоматически агрегируются эти подсети.
	</p>
	<p>
		Данные из Реестра Роскомнадзора подгружаются из сервисов
		<a href="https://antifilter.download/">antifilter.download</a>
		и
		<a href="https://antifilter.network/">antifilter.network</a>
		.
	</p>

	<KbHeading id="что-такое-белый-список">Что такое «белый список»?</KbHeading>
	<p>
		РКН формирует свой белый список доменов, которые доступны, даже если их
		адреса находятся в списке заблокированных подсетей.
	</p>
	<a href="/kb/whitelist">Подробнее...</a>

	<KbHeading id="мне-написало-что-сайт-заблокирован-но-у-меня-все-работает">
		Мне написало, что сайт «Заблокирован», но у меня все работает!
	</KbHeading>
	<p>
		Мы не можем с точностью утверждать, заблокирован сайт или нет. Блокировки
		Роскомнадзора, зачастую, непредсказуемые, непубличные и незаконные.
		Например, блокировка CDN-сервисов, таких, как
		<a
			href="https://blog.cloudflare.com/russian-internet-users-are-unable-to-access-the-open-internet/"
		>
			Cloudflare,
		</a>
		никак не комментировалась официально и не публиковались в Реестре.
		Аналогичная ситуация с
		<a href="https://ria.ru/20241219/putin-1990168675.html">YouTube,</a>
		информация о блокировке которого, не добавлена в Реестр &mdash; для таких
		случаев мы создали
		<a
			href="https://github.com/LowderPlay/cheburcheck/blob/master/querying/dist-domains.txt"
		>
			свой список
		</a>
		подобных сайтов. Однако, список не исчерпывающий, поэтому
		<a href="https://github.com/LowderPlay/cheburcheck/pulls">
			принимаются правки.
		</a>
	</p>
	<p>
		Сайты помечаются как "заблокированные", в случае, если они были найдены хотя
		бы в одном списке.
	</p>

	<KbHeading id="как-часто-обновляются-списки">
		Как часто обновляются списки?
	</KbHeading>
	<p>Мы обновляем списки из источников раз в 6 часов.</p>
	<p>Сами источники обновляют их с разной периодичностью:</p>
	<ul class="list-disc">
		<li>
			<a href="https://github.com/123jjck/cdn-ip-ranges">CDN</a>
			- раз в 12 часов.
		</li>
		<li>
			<a href="/kb/whitelist">Белые списки</a>
			- топ-100,000 раз в 6 часов; топ-1,000,000 раз в сутки.
		</li>
		<li>
			<a href="https://antifilter.download/">Домены из Реестра РКН</a>
			- раз в полчаса.
		</li>
		<li>
			<a href="https://antifilter.network/">Подсети из Реестра РКН</a>
			- раз в час.
		</li>
	</ul>

	<KbHeading id="как-я-могу-помочь-в-развитии-сайта">
		Как я могу помочь в развитии сайта?
	</KbHeading>
	<p>Самый простой способ помочь - рассказать о сайте знакомым.</p>
	<p>
		Если у вас есть навыки разработки, то мы будем рады принять ваши
		pull-requests на
		<a href="https://github.com/LowderPlay/cheburcheck">GitHub.</a>
	</p>
	<p>Если вы хотите помочь финансово:</p>
	<ul class="list-disc">
		<li>TON: <code>UQAACsiwpGryjP-kqp4TJPAWpXytuB6M_puuO0Cg5zNvaSJW</code></li>
	</ul>
	<p>Большое спасибо!</p>
</KbArticle>
