<script lang="ts">
import {
	Copy,
	Download,
	RadioTower,
	RefreshCw,
	Route,
	Trash2,
} from "@lucide/svelte";
import { formatDate } from "$lib/admin/format";
import { createAdminPage } from "$lib/admin/page.svelte";
import { emptyForm, probeForm } from "$lib/admin/probes";
import type {
	CommandResult,
	CommandType,
	Probe,
	ProbeForm,
} from "$lib/api/adminModels";
import AdminPage from "$lib/components/admin/AdminPage.svelte";
import ProbeCommandResult from "$lib/components/admin/ProbeCommandResult.svelte";
import ProbeEditor from "$lib/components/admin/ProbeEditor.svelte";
import ProbeList from "$lib/components/admin/ProbeList.svelte";

const admin = createAdminPage<Probe>("probes", "/probes", () => {
	created = null;
	result = null;
});
let form = $state<ProbeForm>(emptyForm());
let editing = $state<number | null>(null);
let created = $state<{ id: number; token: string } | null>(null);
let selected = $state<number | null>(null);
let target = $state("");
let maxHops = $state(30);
let traceType = $state<"traceroute" | "sni_traceroute">("traceroute");
let sni = $state("");
let result = $state<CommandResult | null>(null);
const probes = $derived(admin.query.data ?? []);
const loading = $derived(admin.query.isFetching);

async function save() {
	if (!form.name.trim()) {
		admin.error = "Укажите имя сканера";
		return;
	}
	await admin.run<Probe | { id: number; token: string }>(
		"save",
		(request) =>
			editing === null
				? request<{ id: number; token: string }>("/probes", "POST", form)
				: request<Probe>(`/probes/${editing}`, "PUT", form),
		async (saved) => {
			if ("token" in saved) {
				created = saved;
				admin.notice = `Сканер #${saved.id} создан. Сохраните данные для подключения.`;
			} else {
				admin.notice = `Сканер #${saved.id} обновлён`;
			}
			form = emptyForm();
			editing = null;
			await admin.refresh();
		},
	);
}
function edit(probe: Probe) {
	editing = probe.id;
	form = probeForm(probe);
	document.getElementById("probe-form")?.scrollIntoView({ behavior: "smooth" });
}
async function toggle(
	probe: Probe,
	key: "hidden" | "disable_traceroutes" | "cdn_unblocked",
) {
	await admin.run(
		`${probe.id}:${key}`,
		(request) =>
			request<Probe>(`/probes/${probe.id}`, "PUT", {
				...probeForm(probe),
				[key]: !probe[key],
			}),
		() => admin.refresh(),
	);
}
async function removeConfirmed(probe: Probe) {
	if (
		!window.confirm(
			"Удалить сканер #" +
				probe.id +
				" (" +
				probe.name +
				")? Связанные результаты сканера будут удалены. Обычные отчёты сохранятся без привязки к сканеру.",
		)
	)
		return;
	await admin.run(
		`${probe.id}:remove`,
		(request) => request(`/probes/${probe.id}`, "DELETE"),
		async () => {
			if (editing === probe.id) {
				editing = null;
				form = emptyForm();
			}
			if (selected === probe.id) {
				selected = null;
				result = null;
			}
			if (created?.id === probe.id) created = null;
			admin.notice = `Сканер #${probe.id} удалён`;
			await admin.refresh();
		},
	);
}
async function reloadConfig() {
	await admin.run(
		"reload",
		(request) =>
			request<{ hosts: unknown[]; published_at: string }>(
				"/probe-config/reload",
				"POST",
			),
		(config) => {
			admin.notice = `Конфигурация отправлена: ${config.hosts.length} хостов, ${formatDate(config.published_at)}. Кеш ответов очищен.`;
		},
	);
}
async function clearCache() {
	await admin.run(
		"clear-cache",
		(request) => request<{ removed: number }>("/probe-cache/clear", "POST"),
		({ removed }) => {
			admin.notice = `Кеш ответов очищен. Удалено записей: ${removed}.`;
		},
	);
}
async function updateCheck(id: number | null) {
	await admin.run(
		id === null ? "update:all" : `${id}:update`,
		(request) =>
			request(
				id === null ? "/probes/update-check" : `/probes/${id}/update-check`,
				"POST",
			),
		() => {
			admin.notice =
				id === null
					? "Запрос проверки обновлений отправлен всем подключённым сканерам"
					: `Запрос проверки обновлений отправлен сканеру #${id}`;
		},
	);
}
async function command(id: number, type: CommandType) {
	if (admin.busy) return;
	selected = id;
	result = null;
	await admin.run(
		`${id}:command`,
		(request) =>
			request<CommandResult>(
				`/probes/${id}/commands`,
				"POST",
				type === "traceroute"
					? { type, target: target.trim(), max_hops: maxHops }
					: type === "sni_traceroute"
						? { type, host: target.trim(), sni: sni.trim(), max_hops: maxHops }
						: { type },
			),
		async (response) => {
			result = response;
			if (response.type === "remeasure_dpi_hop") await admin.refresh();
		},
	);
}
async function copy(value: string) {
	await admin.copy(value);
}
</script>

<AdminPage title="Сканеры" {admin}>
	<div class="flex flex-wrap gap-3">
		<button
			type="button"
			class="btn flex items-center gap-2"
			onclick={() => void admin.query.refetch()}
			disabled={loading}
		>
			<RefreshCw size={16} />
			Обновить список
		</button><button
			type="button"
			class="btn flex items-center gap-2"
			onclick={() => void reloadConfig()}
			disabled={admin.busy !== ""}
		>
			<RadioTower size={16} />
			Перезагрузить probe-hosts.toml
		</button>
		<button
			type="button"
			class="btn flex items-center gap-2"
			onclick={() => void clearCache()}
			disabled={admin.busy !== ""}
		>
			<Trash2 size={16} />
			{admin.busy === "clear-cache" ? "Очистка кеша…" : "Очистить кеш ответов"}
		</button>
		<button
			type="button"
			class="btn flex items-center gap-2"
			onclick={() => void updateCheck(null)}
			disabled={admin.busy !== ""}
		>
			<Download size={16} />
			Проверить обновления всех
		</button>
	</div>

	<ProbeList
		{probes}
		{loading}
		busy={admin.busy}
		onedit={edit}
		ontoggle={toggle}
		oncommand={command}
		onupdate={updateCheck}
		onremove={removeConfirmed}
		ontrace={(id) => {
 selected = id;
 result = null;
 document.getElementById("commands")?.scrollIntoView({ behavior: "smooth" });
 }}
	/>

	{#if created}
		<section class="panel border-emerald-800">
			<h2 class="font-semibold text-emerald-300">Данные нового сканера</h2>
			<p class="mt-1 text-xs text-neutral-400">
				Токен показан только сейчас. Скопируйте его перед закрытием страницы.
			</p>
			<div class="mt-4 flex items-start gap-3">
				<pre
					class="min-w-0 flex-1 overflow-x-auto rounded-lg bg-black/60 p-4 text-sm text-cyan-200"
				>PROBE_ID={created.id}
PROBE_TOKEN={created.token}</pre>
				<button
					type="button"
					class="btn"
					aria-label="Скопировать переменные"
					onclick={() => void copy(`PROBE_ID=${created?.id}\nPROBE_TOKEN=${created?.token}`)}
				>
					<Copy size={17} />
				</button>
			</div>
		</section>
	{/if}

	<ProbeEditor
		bind:form
		{editing}
		busy={admin.busy}
		onsave={save}
		oncancel={() => { editing = null; form = emptyForm(); }}
	/>

	<section id="commands" class="panel">
		<h2 class="mb-2 text-lg font-semibold">Команды и трассировка</h2>
		<p class="mb-5 text-sm text-neutral-400">
			Команда выполняется выбранным сканером. DPI и SNI измерения могут занять
			несколько минут.
		</p>
		<div class="mb-4 flex flex-wrap items-end gap-3">
			<label class="field"
				>Режим трассировки
				<select class="input" bind:value={traceType}>
					<option value="traceroute">TCP traceroute</option>
					<option value="sni_traceroute">SNI traceroute (DPI)</option>
				</select>
			</label>
			<button
				type="button"
				class="btn"
				disabled={selected === null || admin.busy !== ""}
				onclick={() => selected !== null && void command(selected, "remeasure_dpi_hop")}
			>
				Перемерить DPI hop
			</button>
		</div>
		{#if traceType === "sni_traceroute"}
			<label class="field mb-4"
				>SNI<input
					class="input"
					type="text"
					placeholder="rutracker.org"
					bind:value={sni}
				></label
			>
			<p class="mb-4 text-sm text-neutral-400">
				Для каждого TTL создаётся новое TCP-соединение к хосту на порту 443.
				ClientHello с указанным SNI отправляется с обычным TTL; через 100 мс
				отправляются случайные данные с проверяемым TTL. Хост разрешается
				сканером; используется первый IP адрес.
			</p>
		{/if}
		<div class="grid gap-3 sm:grid-cols-[1fr_2fr_100px_auto] sm:items-end">
			<label class="field"
				>Сканер<select class="input" bind:value={selected}>
					<option value={null}>Выберите</option>
					{#each probes as probe}
						<option value={probe.id}>#{probe.id} {probe.name}</option>
					{/each}
				</select></label
			><label class="field"
				>{traceType === "sni_traceroute" ? "Хост или IP адрес цели" : "IP адрес цели"}<input
					class="input"
					type="text"
					placeholder="1.1.1.1"
					bind:value={target}
				></label
			><label class="field"
				>Макс. hops<input
					class="input"
					type="number"
					min="1"
					max="64"
					bind:value={maxHops}
				></label
			><button
				type="button"
				class="btn-primary flex items-center justify-center gap-2"
				disabled={selected === null || !target.trim() || (traceType === "sni_traceroute" && !sni.trim()) || !Number.isInteger(maxHops) || maxHops < 1 || maxHops > 64 || admin.busy !== ""}
				onclick={() => selected !== null && void command(selected, traceType)}
			>
				<Route size={16} />
				Запустить
			</button>
		</div>
		{#if admin.busy.endsWith(":command")}
			<p class="mt-5 text-sm text-cyan-300">Ожидание ответа сканера…</p>
		{/if}
		{#if result && result.type !== "error"}
			<button
				type="button"
				class="btn mt-4"
				onclick={() => result && void copy(JSON.stringify(result, null, 2))}
			>
				Копировать JSON результата
			</button>
		{/if}
		<ProbeCommandResult {result} />
	</section>
</AdminPage>
