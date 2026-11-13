<script lang="ts">
import KbApiRequest from "$lib/components/kb/KbApiRequest.svelte";
import KbArticle from "$lib/components/kb/KbArticle.svelte";
import KbHeading from "$lib/components/kb/KbHeading.svelte";
import KbNote from "$lib/components/kb/KbNote.svelte";

const checkResponse = JSON.stringify(
	{
		id: "<подписанный id>",
		target: "example.com",
		target_type: "Домен",
		blocked: false,
		ips: ["93.184.215.14"],
		reverse_lookup: [],
		blocked_subnets: [],
		cdn_providers: {},
		geo: { location: "США" },
		complaints: [],
	},
	null,
	2,
);
const statusResponse = JSON.stringify(
	{
		domain_count: 1200000,
		v4_count: 350000,
		last_update: "2026-11-06T00:00:00Z",
		version: "1.5.7",
	},
	null,
	2,
);
const apiHeaders = { Authorization: "Bearer <API-токен>" };
const streamExample = `event: started
    data: {"id":"…","target":"example.com","online_probes":2}

    event: result
    data: {"job_id":"…","probe_id":"42","cached":false,"verdicts":["ok"],"host_results":[],"target_hop":null,"dpi_hop":null,"dns":null}

    event: done
    data: {"id":"…","status":"done","response_count":2,"online_probes":2}`;
</script>

<svelte:head>
	<title>Документация API — Cheburcheck</title>
	<meta
		name="description"
		content="API Cheburcheck: проверка блокировок, сетевые сканеры через SSE и авторизация по API-токену."
	>
	<meta property="og:title" content="Документация API — Cheburcheck">
	<meta property="og:url" content="https://cheburcheck.ru/kb/api">
</svelte:head>

<KbArticle>
	<h1>Документация API</h1>
	<p>
		Базовый адрес API: <code>https://cheburcheck.ru/api/v1</code>. Проверка по
		спискам возвращает JSON, а сетевые сканеры передают результаты по мере
		готовности через Server-Sent Events (SSE).
	</p>

	<KbHeading id="authorization" title="Авторизация">Авторизация</KbHeading>
	<KbNote variant="warning"
		>API-токены выдаются администратором по заявке. Чтобы получить токен для
		запросов к публичным сканерам, заполните
		<a href="https://forms.gle/LSPcAV5n3CrdQLMP6">форму заявки</a>. При
		злоупотреблении токен может быть отозван.
	</KbNote>
	<p>
		Для <code>GET /check</code> и <code>GET /status</code> авторизация не
		требуется. Для <code>GET /probe/&lt;id&gt;</code> передайте API-токен в
		заголовке <code>Authorization: Bearer &lt;токен&gt;</code>. Для запроса к
		одному сканеру достаточно его токена в параметре
		<code>token</code>; API-токен в этом случае не требуется.
	</p>
	<p>
		Храните API-токен на сервере или в секретах вашей системы автоматизации.
		Передавайте его в заголовке, а не в URL и не в публичном коде сайта.
		API-токен для сканеров не даёт доступ к административным методам.
	</p>

	<KbHeading id="check" title="GET /check — проверка ресурса"
		>GET /check — проверка ресурса</KbHeading
	>
	<p>
		<code>GET /check?target=&lt;цель&gt;</code>
		принимает домен, IPv4, IPv6, подсеть в формате CIDR или ASN (например,
		<code>AS13335</code>).
	</p>
	<KbApiRequest
		url="https://cheburcheck.ru/api/v1/check?target=example.com"
		responseBody={checkResponse}
	/>
	<p>Основные поля JSON-ответа:</p>
	<ul>
		<li>
			<code>id</code>
			— контекст для сканеров и обратной связи; может быть
			<code>null</code>.
		</li>
		<li>
			<code>target</code>, <code>target_type</code> — нормализованная цель и её
			тип.
		</li>
		<li>
			<code>blocked</code>, <code>rkn_domain</code>,
			<code>blocked_subnets</code>
			— результат проверки по спискам блокировок.
		</li>
		<li>
			<code>ips</code>, <code>reverse_lookup</code>, <code>geo</code> — адреса,
			обратные DNS-имена и данные о сети.
		</li>
		<li>
			<code>cdn_providers</code>
			— сети CDN по провайдерам; <code>whitelist</code> — найденное исключение.
		</li>
		<li>
			<code>asn_info</code>, <code>subnet_size</code> — дополнительные сведения
			для ASN и подсетей.
		</li>
		<li>
			<code>complaints</code>
			— дневные счётчики жалоб в виде объектов с полями <code>date</code>
			и
			<code>count</code>.
		</li>
	</ul>
	<p>
		Необязательные поля могут отсутствовать или содержать <code>null</code>.
		Методика работы сканеров приведена в
		<a href="/kb/probing">отдельной статье</a>.
	</p>

	<KbNote variant="info"
		>Поле <code>id</code> — подписанный контекст проверки со сроком действия 24
		часа. Передавайте его целиком.</KbNote
	>

	<KbHeading id="probe" title="GET /probe/&lt;id&gt; — сетевые сканеры"
		>GET /probe/&lt;id&gt; — сетевые сканеры</KbHeading
	>
	<p>
		<code>GET /probe/&lt;id&gt;</code>
		возвращает поток <code>text/event-stream</code>. Сканеры доступны для
		доменов и отдельных IP-адресов. Подсети, ASN и специальные локальные адреса
		не поддерживаются.
	</p>
	<p>
		Сначала выполните <a href="#check">GET /check</a> и подставьте поле
		<code>id</code>
		из ответа в URL запроса. Передайте API-токен в заголовке
		<code>Authorization: Bearer &lt;токен&gt;</code>
		и читайте события SSE до события <code>done</code>.
	</p>
	<KbNote variant="info">
		Лимиты API можно снять по запросу. Для этого напишите на
		<a href="mailto:support@cheburcheck.ru">support@cheburcheck.ru</a>.
	</KbNote>
	<p>
		Необязательный параметр <code>token</code> позволяет выбрать конкретный
		сканер. Без выбора сканера используются доступные публичные сканеры.
	</p>
	<p>Обрабатывайте именованные события:</p>
	<ul>
		<li>
			<code>started</code>
			— поля <code>id</code>, <code>target</code>,
			<code>online_probes</code>: начало запроса и число ожидаемых результатов.
		</li>
		<li>
			<code>result</code>
			— один ответ сканера. Сопоставляйте результаты по
			<code>probe_id</code>.
		</li>
		<li>
			<code>done</code>
			— поля <code>id</code>, <code>response_count</code>,
			<code>online_probes</code>: завершение ожидания. После него закройте
			соединение.
		</li>
	</ul>
	<p>Сокращённый пример потока (показан один из двух результатов):</p>
	<KbApiRequest
		url="https://cheburcheck.ru/api/v1/probe/<id>"
		headers={apiHeaders}
		responseType="text/event-stream"
		responseBody={streamExample}
	/>
	<p>
		В <code>result</code> также могут присутствовать <code>region</code>,
		<code>provider</code>, <code>asn</code>, <code>cdn_unblocked</code>,
		сведения о хостах в <code>host_results</code>, DNS в <code>dns</code> и
		прыжках в <code>target_hop</code> и <code>dpi_hop</code>. Поле
		<code>cached: true</code>
		означает повторное использование сохранённого измерения; по умолчанию кеш
		действует три часа.
	</p>
	<p>
		Массив <code>verdicts</code> содержит значения <code>ok</code>,
		<code>uncertain</code>, <code>cdn_block</code>, <code>sni_block</code>,
		<code>dns_spoofing</code>, <code>tspu_block</code> или
		<code>whitelist</code>. Один сканер может вернуть несколько признаков.
	</p>
	<p>
		Событие <code>done</code> приходит и при истечении времени ожидания, поэтому
		<code>response_count</code>
		может быть меньше <code>online_probes</code>.
	</p>

	<KbHeading id="credits" title="GET /credits — баланс кредитов"
		>GET /credits — баланс кредитов</KbHeading
	>
	<p>
		Передайте API-токен в заголовке
		<code>Authorization: Bearer &lt;токен&gt;</code>. Ответ
		<code>{JSON.stringify({ credits: 42 })}</code>
		содержит текущий баланс этого токена. Запрос не расходует кредиты и работает
		при нулевом балансе. Отсутствующий, неверный, просроченный или отозванный
		токен возвращает
		<code>401</code>.
	</p>

	<KbHeading id="other-methods" title="GET /status — статус сервиса"
		>GET /status — статус сервиса</KbHeading
	>
	<p>
		<code>GET /status</code>
		возвращает JSON с количеством доменов (<code>domain_count</code>) и
		IPv4-адресов (<code>v4_count</code>) в списках, временем последнего
		обновления (<code>last_update</code>) и версией сервиса (<code
			>version</code
		>). Авторизация не требуется.
	</p>
	<KbApiRequest
		url="https://cheburcheck.ru/api/v1/status"
		responseBody={statusResponse}
	/>

	<KbHeading id="healthcheck" title="GET /healthcheck — готовность сервиса"
		>GET /healthcheck — готовность сервиса</KbHeading
	>
	<p>
		<code>GET /healthcheck</code>
		возвращает текст <code>OK</code> с кодом
		<code>200</code>, когда списки загружены. До загрузки списков возвращается
		код <code>500</code>.
	</p>
	<KbApiRequest
		url="https://cheburcheck.ru/api/v1/healthcheck"
		responseType="text/plain"
		responseBody="OK"
	/>

	<KbHeading
		id="feedback"
		title="POST /feedback/&lt;id&gt;/&lt;works&gt; — обратная связь"
		>POST /feedback/&lt;id&gt;/&lt;works&gt; — обратная связь</KbHeading
	>
	<p>
		<code>POST /feedback/&lt;id&gt;/&lt;works&gt;</code>
		отправляет обратную связь по проверке. Подставьте <code>id</code> из ответа
		<a href="#check">GET /check</a>, а вместо <code>works</code> передайте
		<code>true</code>, если ресурс работает, или <code>false</code>, если
		недоступен. При успехе возвращается код <code>200</code> без тела ответа.
	</p>
	<KbApiRequest
		method="POST"
		url="https://cheburcheck.ru/api/v1/feedback/<id>/true"
		responseType="—"
	/>

	<KbHeading id="errors" title="Ошибки и лимиты">Ошибки и лимиты</KbHeading>
	<p>
		До открытия SSE ошибки возвращаются как JSON с полями <code>code</code>
		и
		<code>info</code>. При отсутствии авторизации ответ 403 также содержит поле
		<code>documentation_url</code>
		со ссылкой на эту документацию.
	</p>
	<ul>
		<li>
			<code>400</code>
			— неподдерживаемая цель для сканеров, отключённая трассировка IP или
			неверные параметры.
		</li>
		<li>
			<code>401</code>
			— неверный или просроченный контекст <code>id</code>, неверный,
			просроченный или отозванный API-токен либо неверный заголовок авторизации.
		</li>
		<li>
			<code>403</code>
			— отсутствует авторизация для сканеров или целевой адрес запрещён.
		</li>
		<li>
			<code>404</code>
			— ресурс не найден при проверке или указанный сканер не существует.
		</li>
		<li>
			<code>429</code>
			— превышен лимит запросов. Уменьшите частоту и повторите запрос позже.
		</li>
		<li>
			<code>500</code>, <code>503</code>, <code>504</code> — ошибка сервиса,
			недоступность зависимостей или таймаут. Повторите запрос позже.
		</li>
	</ul>
	<p>
		Для API действуют ограничения частоты запросов. Чтобы снять их, напишите на
		<a href="mailto:support@cheburcheck.ru">support@cheburcheck.ru</a>.
	</p>
</KbArticle>
