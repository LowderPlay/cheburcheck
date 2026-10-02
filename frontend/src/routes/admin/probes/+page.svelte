<script lang="ts">
import {
	Copy,
	Download,
	Plus,
	Radio,
	RadioTower,
	RefreshCw,
	Route,
	Settings2,
	Trash2,
} from "@lucide/svelte";
import {
	createMutation,
	createQuery,
	useQueryClient,
} from "@tanstack/svelte-query";
import { onMount } from "svelte";
import { adminRequest } from "$lib/api/admin";

type Probe = {
	id: number;
	name: string;
	region: string | null;
	asn: string | null;
	provider: string | null;
	hidden: boolean;
	disable_traceroutes: boolean;
	cdn_unblocked: boolean;
	last_connected_at: string | null;
	online: boolean;
	version: string | null;
	bundle_type: string | null;
	dpi_hop_v4: number | null;
	dpi_hop_v6: number | null;
};
type Hop = {
	ttl: number;
	address: string | null;
	reverse_names: string[];
	outcome: string;
};
type DpiHop = { ttl: number; src: string | null; outcome: string };
type CommandType =
	| "resubscribe_tasks"
	| "traceroute"
	| "remeasure_dpi_hop"
	| "sni_traceroute";
type CommandResult =
	| { type: "traceroute"; target: string; hops: Hop[] }
	| { type: "resubscribe_tasks"; requested: boolean }
	| {
			type: "remeasure_dpi_hop";
			dpi_hop_v4: number | null;
			dpi_hop_v6: number | null;
			dpi_hops_v4: DpiHop[];
			dpi_hops_v6: DpiHop[];
	  }
	| {
			type: "sni_traceroute";
			host: string;
			sni: string;
			target: string;
			dpi_hop: number | null;
			hops: DpiHop[];
	  }
	| { type: "error"; message: string };
type Form = Pick<
	Probe,
	| "name"
	| "region"
	| "asn"
	| "provider"
	| "hidden"
	| "disable_traceroutes"
	| "cdn_unblocked"
>;
const emptyForm = (): Form => ({
	name: "",
	region: "",
	asn: "",
	provider: "",
	hidden: false,
	disable_traceroutes: false,
	cdn_unblocked: false,
});
const passwordKey = "cheburcheck:admin-password";
let password = $state("");
let enteredPassword = $state("");
let signingIn = $state(false);
let onlineOnly = $state(false);
let busy = $state("");
let error = $state("");
let notice = $state("");
let form = $state<Form>(emptyForm());
let editing = $state<number | null>(null);
let created = $state<{ id: number; token: string } | null>(null);
let selected = $state<number | null>(null);
let target = $state("");
let maxHops = $state(30);
let traceType = $state<"traceroute" | "sni_traceroute">("traceroute");
let sni = $state("");
let result = $state<CommandResult | null>(null);

const queryClient = useQueryClient();
const probeKey = ["admin", "probes"] as const;
const probesQuery = createQuery(() => ({
	queryKey: probeKey,
	queryFn: () => api<Probe[]>("/probes"),
	enabled: password.length > 0,
	staleTime: 15_000,
	refetchInterval: 30_000,
}));
const probes = $derived(probesQuery.data ?? []);
const sortedProbes = $derived(
	[...probes].sort(
		(a, b) => Number(b.online) - Number(a.online) || a.id - b.id,
	),
);
const visibleProbes = $derived(
	onlineOnly ? sortedProbes.filter((probe) => probe.online) : sortedProbes,
);
const loading = $derived(probesQuery.isFetching);
const createProbe = createMutation(() => ({
	mutationFn: (input: Form) =>
		api<{ id: number; token: string }>("/probes", "POST", input),
	onSuccess: () => queryClient.invalidateQueries({ queryKey: probeKey }),
}));
const updateProbe = createMutation(() => ({
	mutationFn: ({ id, input }: { id: number; input: Form }) =>
		api<Probe>("/probes/" + id, "PUT", input),
	onSuccess: () => queryClient.invalidateQueries({ queryKey: probeKey }),
}));
const removeProbe = createMutation(() => ({
	mutationFn: (id: number) =>
		api<{ removed: boolean }>("/probes/" + id, "DELETE"),
	onSuccess: () => queryClient.invalidateQueries({ queryKey: probeKey }),
}));
const reloadProbeConfig = createMutation(() => ({
	mutationFn: () =>
		api<{ hosts: unknown[]; published_at: string }>(
			"/probe-config/reload",
			"POST",
		),
}));
const requestUpdateCheck = createMutation(() => ({
	mutationFn: (id: number | null) =>
		api<{ requested: boolean }>(
			id === null ? "/probes/update-check" : "/probes/" + id + "/update-check",
			"POST",
		),
}));
const sendProbeCommand = createMutation(() => ({
	mutationFn: ({
		id,
		type,
		target,
		maxHops,
		sni,
	}: {
		id: number;
		type: CommandType;
		target: string;
		maxHops: number;
		sni: string;
	}) =>
		api<CommandResult>(
			"/probes/" + id + "/commands",
			"POST",
			type === "traceroute"
				? { type, target, max_hops: maxHops }
				: type === "sni_traceroute"
					? { type, host: target, sni, max_hops: maxHops }
					: { type },
		),
}));
$effect(() => {
	if (probesQuery.isError)
		error = String(
			probesQuery.error instanceof Error
				? probesQuery.error.message
				: probesQuery.error,
		);
});

function api<T>(
	path: string,
	method = "GET",
	body?: unknown,
	auth = password,
): Promise<T> {
	return adminRequest<T>(auth, path, method, body);
}

async function signIn() {
	const candidate = enteredPassword.trim();
	error = "";
	signingIn = true;
	try {
		queryClient.removeQueries({ queryKey: ["admin", "login"] });
		const rows = await queryClient.fetchQuery({
			queryKey: ["admin", "login"],
			queryFn: () => api<Probe[]>("/probes", "GET", undefined, candidate),
		});
		queryClient.setQueryData(probeKey, rows);
		password = candidate;
		localStorage.setItem(passwordKey, candidate);
	} catch (e) {
		error = String(e instanceof Error ? e.message : e);
	} finally {
		queryClient.removeQueries({ queryKey: ["admin", "login"] });
		signingIn = false;
	}
}
function signOut() {
	localStorage.removeItem(passwordKey);
	password = "";
	enteredPassword = "";
	queryClient.removeQueries({ queryKey: probeKey });
	created = null;
	result = null;
	error = "";
}
onMount(() => {
	const saved = localStorage.getItem(passwordKey);
	if (saved) {
		password = saved;
		enteredPassword = saved;
	}
});

async function save() {
	if (!form.name.trim()) {
		error = "Укажите имя сканера";
		return;
	}
	busy = "save";
	error = "";
	notice = "";
	try {
		if (editing === null) {
			created = await createProbe.mutateAsync(form);
			notice = `Сканер #${created.id} создан. Сохраните данные для подключения.`;
		} else {
			await updateProbe.mutateAsync({ id: editing, input: form });
			notice = `Сканер #${editing} обновлён`;
		}
		form = emptyForm();
		editing = null;
	} catch (e) {
		error = String(e instanceof Error ? e.message : e);
	} finally {
		busy = "";
	}
}
function edit(probe: Probe) {
	editing = probe.id;
	form = {
		name: probe.name,
		region: probe.region,
		asn: probe.asn,
		provider: probe.provider,
		hidden: probe.hidden,
		disable_traceroutes: probe.disable_traceroutes,
		cdn_unblocked: probe.cdn_unblocked,
	};
	document.getElementById("probe-form")?.scrollIntoView({ behavior: "smooth" });
}
async function toggle(
	probe: Probe,
	key: "hidden" | "disable_traceroutes" | "cdn_unblocked",
) {
	busy = `${probe.id}:${key}`;
	error = "";
	try {
		await updateProbe.mutateAsync({
			id: probe.id,
			input: {
				name: probe.name,
				region: probe.region,
				asn: probe.asn,
				provider: probe.provider,
				hidden: probe.hidden,
				disable_traceroutes: probe.disable_traceroutes,
				cdn_unblocked: probe.cdn_unblocked,
				[key]: !probe[key],
			},
		});
	} catch (e) {
		error = String(e instanceof Error ? e.message : e);
	} finally {
		busy = "";
	}
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
	busy = probe.id + ":remove";
	error = "";
	notice = "";
	try {
		await removeProbe.mutateAsync(probe.id);
		if (editing === probe.id) {
			editing = null;
			form = emptyForm();
		}
		if (selected === probe.id) {
			selected = null;
			result = null;
		}
		if (created?.id === probe.id) created = null;
		notice = "Сканер #" + probe.id + " удалён";
	} catch (e) {
		error = String(e instanceof Error ? e.message : e);
	} finally {
		busy = "";
	}
}
async function reloadConfig() {
	busy = "reload";
	error = "";
	notice = "";
	try {
		const config = await reloadProbeConfig.mutateAsync();
		notice = `Конфигурация отправлена: ${config.hosts.length} хостов, ${new Date(config.published_at).toLocaleString("ru-RU")}`;
	} catch (e) {
		error = String(e instanceof Error ? e.message : e);
	} finally {
		busy = "";
	}
}
async function updateCheck(id: number | null) {
	busy = id === null ? "update:all" : id + ":update";
	error = "";
	notice = "";
	try {
		await requestUpdateCheck.mutateAsync(id);
		notice =
			id === null
				? "Запрос проверки обновлений отправлен всем подключённым сканерам"
				: "Запрос проверки обновлений отправлен сканеру #" + id;
	} catch (e) {
		error = String(e instanceof Error ? e.message : e);
	} finally {
		busy = "";
	}
}
async function command(id: number, type: CommandType) {
	selected = id;
	result = null;
	busy = `${id}:command`;
	error = "";
	try {
		result = await sendProbeCommand.mutateAsync({
			id,
			type,
			target: target.trim(),
			maxHops,
			sni: sni.trim(),
		});
		if (result.type === "remeasure_dpi_hop") {
			await queryClient.invalidateQueries({ queryKey: probeKey });
		}
	} catch (e) {
		error = String(e instanceof Error ? e.message : e);
	} finally {
		busy = "";
	}
}
async function copy(value: string) {
	await navigator.clipboard.writeText(value);
	notice = "Скопировано";
}
function asnTarget(asn: string): string {
	const value = asn.trim().toUpperCase();
	return value.startsWith("AS") ? value : "AS" + value;
}
const outcomeLabel: Record<string, string> = {
	icmp_time_exceeded: "Промежуточный узел",
	rst: "TCP RST",
	connected: "Подключено",
	tcp_closed: "TCP закрыт",
	tcp_acknowledged: "TCP подтверждён",
	timeout: "Нет ответа",
};
</script>

{#snippet dpiHops(hops: DpiHop[])}
	<ol class="space-y-1">
		{#each hops as hop}
			<li
				class="grid grid-cols-[2.5rem_1fr_auto] gap-3 rounded-lg border border-neutral-800 bg-black/20 px-3 py-2 text-sm"
			>
				<span class="font-mono text-neutral-500">{hop.ttl}</span>
				<span class="font-mono">{hop.src ?? "* * *"}</span>
				<span class="text-xs text-neutral-500"
					>{outcomeLabel[hop.outcome] ?? hop.outcome}</span
				>
			</li>
		{/each}
	</ol>
	{#if hops.length === 0}
		<p class="text-sm text-neutral-500">
			Измерение не удалось: ответов нет. Подробности в логах сканера.
		</p>
	{/if}
{/snippet}

<svelte:head
	><title>Сканеры · Cheburcheck</title>
	<meta name="robots" content="noindex, nofollow"></svelte:head
>

<div class="space-y-7 text-neutral-100">
	<div class="flex flex-wrap items-end justify-between gap-4">
		<h1 class="mt-2 text-3xl font-bold tracking-tight">Сканеры</h1>
		{#if password}
			<button type="button" class="btn" onclick={signOut}>Выйти</button>
		{/if}
	</div>

	{#if !password || error === "Неверный пароль администратора"}
		<form
			class="panel max-w-md space-y-4"
			onsubmit={(event) => { event.preventDefault(); void signIn(); }}
		>
			<label class="block text-sm font-medium" for="admin-password"
				>Пароль администратора</label
			>
			<input
				id="admin-password"
				class="input"
				type="password"
				autocomplete="current-password"
				bind:value={enteredPassword}
				required
			>
			{#if error}
				<p role="alert" class="text-sm text-red-300">{error}</p>
			{/if}
			<button type="submit" class="btn-primary" disabled={signingIn}>
				{signingIn ? "Проверка…" : "Открыть"}
			</button>
		</form>
	{:else}
		<div class="flex flex-wrap gap-3">
			<button
				type="button"
				class="btn flex items-center gap-2"
				onclick={() => void probesQuery.refetch()}
				disabled={loading}
			>
				<RefreshCw size={16} />
				Обновить список
			</button><button
				type="button"
				class="btn flex items-center gap-2"
				onclick={() => void reloadConfig()}
				disabled={busy !== ""}
			>
				<RadioTower size={16} />
				Перезагрузить probe-hosts.toml
			</button>
			<button
				type="button"
				class="btn flex items-center gap-2"
				onclick={() => void updateCheck(null)}
				disabled={busy !== ""}
			>
				<Download size={16} />
				Проверить обновления всех
			</button>
		</div>
		{#if notice}
			<div
				role="status"
				class="rounded-lg border border-emerald-800 bg-emerald-950/40 px-4 py-3 text-sm text-emerald-300"
			>
				{notice}
			</div>
		{/if}
		{#if error}
			<div
				role="alert"
				class="rounded-lg border border-red-800 bg-red-950/40 px-4 py-3 text-sm text-red-300"
			>
				{error}
			</div>
		{/if}

		<section class="panel overflow-x-auto">
			<div class="mb-4 flex flex-wrap items-center justify-between gap-3">
				<h2 class="text-lg font-semibold">Текущие сканеры</h2>
				<div class="flex flex-wrap items-center gap-4">
					<label class="check text-xs">
						<input type="checkbox" bind:checked={onlineOnly}>
						Только онлайн
					</label>
					<span class="text-xs text-neutral-500"
						>{probes.length}
						всего · {probes.filter((p) => p.online).length} онлайн</span
					>
				</div>
			</div>
			{#if visibleProbes.length === 0}
				<p class="py-8 text-center text-sm text-neutral-500">
					{loading ? "Загрузка…" : onlineOnly && probes.length > 0 ? "Нет сканеров онлайн" : "Сканеров пока нет"}
				</p>
			{:else}
				<table class="w-full min-w-[780px] text-left text-sm">
					<thead
						class="border-b border-neutral-800 text-xs uppercase tracking-wider text-neutral-500"
					>
						<tr>
							<th class="py-3 pr-3">Сканер</th>
							<th class="px-3">Состояние</th>
							<th class="px-3">Метаданные</th>
							<th class="px-3">Флаги</th>
							<th class="pl-3">Действия</th>
						</tr>
					</thead>
					<tbody>
						{#each visibleProbes as probe (probe.id)}
							<tr
								class="border-b border-neutral-800/70 align-top last:border-0"
							>
								<td class="py-4 pr-3">
									<div class="font-semibold">#{probe.id}</div>
									<div class="mt-1 text-xs text-neutral-500">
										<p>{probe.name}</p>
										{probe.last_connected_at ? `${new Date(probe.last_connected_at).toLocaleString("ru-RU")}` : "Не подключался"}
									</div>
								</td>
								<td class="px-3 py-4">
									<span
										class:!text-emerald-300={probe.online}
										class="inline-flex items-center gap-1 text-neutral-500"
										><Radio size={14} />
										{probe.online ? "Онлайн" : "Офлайн"}</span
									>
									<div class="mt-1 text-xs text-neutral-500">
										{probe.version ?? "Версия неизвестна"}
										{probe.bundle_type ? `· ${probe.bundle_type}` : ""}
									</div>
									{#if probe.dpi_hop_v4 !== null || probe.dpi_hop_v6 !== null}
										<div class="mt-1 text-xs text-neutral-500">
											v4: <b>{probe.dpi_hop_v4 ?? "—"}</b>; v6:
											<b>{probe.dpi_hop_v6 ?? "—"}</b>
										</div>
									{/if}
								</td>
								<td class="px-3 py-4 text-xs text-neutral-400">
									{probe.region || "Регион не указан"}<br>
									{probe.provider || "Провайдер не указан"}
									{#if probe.asn}
										·
										<a
											class="text-cyan-300 underline hover:text-cyan-200"
											href={"/check?target=" + encodeURIComponent(asnTarget(probe.asn))}
											>{probe.asn}</a
										>
									{/if}
								</td>
								<td class="px-3 py-4">
									<div class="flex flex-col items-start gap-1">
										<button
											type="button"
											class="flag"
											class:active={probe.hidden}
											disabled={busy !== ""}
											onclick={() => void toggle(probe, "hidden")}
										>
											{probe.hidden ? "Скрыт" : "Публичный"}
										</button><button
											type="button"
											class="flag"
											class:active={probe.disable_traceroutes}
											disabled={busy !== ""}
											onclick={() => void toggle(probe, "disable_traceroutes")}
										>
											{probe.disable_traceroutes ? "Трассировка выкл." : "Трассировка вкл."}
										</button><button
											type="button"
											class="flag"
											class:active={probe.cdn_unblocked}
											disabled={busy !== ""}
											onclick={() => void toggle(probe, "cdn_unblocked")}
										>
											{probe.cdn_unblocked ? "CDN доступен" : "CDN блокируется"}
										</button>
									</div>
								</td>
								<td class="pl-3 py-4">
									<div class="flex flex-col items-start gap-2">
										<button
											type="button"
											class="link"
											onclick={() => edit(probe)}
										>
											<Settings2 size={14} />
											Изменить
										</button><button
											type="button"
											class="link"
											disabled={busy !== ""}
											onclick={() => void command(probe.id, "resubscribe_tasks")}
										>
											<RefreshCw size={14} />
											Переподписать
										</button><button
											type="button"
											class="link"
											disabled={busy !== "" || !probe.online}
											onclick={() => void command(probe.id, "remeasure_dpi_hop")}
										>
											<RefreshCw size={14} />
											Перемерить DPI hop
										</button><button
											type="button"
											class="link"
											disabled={busy !== ""}
											onclick={() => void updateCheck(probe.id)}
										>
											<Download size={14} />
											Проверить обновление
										</button><button
											type="button"
											class="link"
											onclick={() => { selected = probe.id; result = null; document.getElementById("commands")?.scrollIntoView({ behavior: "smooth" }); }}
										>
											<Route size={14} />
											Traceroute
										</button>
										<button
											type="button"
											class="link remove-link"
											disabled={busy !== ""}
											onclick={() => void removeConfirmed(probe)}
										>
											<Trash2 size={14} />
											Удалить
										</button>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</section>

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

		<section id="probe-form" class="panel">
			<h2 class="mb-5 text-lg font-semibold">
				{editing === null ? "Добавить сканер" : `Изменить сканер #${editing}`}
			</h2>
			<form
				class="space-y-4"
				onsubmit={(event) => { event.preventDefault(); void save(); }}
			>
				<div class="grid gap-4 sm:grid-cols-2">
					<label class="field"
						>Имя<input
							class="input"
							bind:value={form.name}
							maxlength="255"
							required
						></label
					><label class="field"
						>Регион<input
							class="input"
							bind:value={form.region}
							maxlength="255"
						></label
					><label class="field"
						>Провайдер<input
							class="input"
							bind:value={form.provider}
							maxlength="255"
						></label
					><label class="field"
						>ASN<input
							class="input"
							bind:value={form.asn}
							maxlength="32"
						></label
					>
				</div>
				<div class="flex flex-wrap gap-5 text-sm">
					<label class="check"
						><input type="checkbox" bind:checked={form.hidden}>
						Скрыт</label
					><label class="check"
						><input type="checkbox" bind:checked={form.disable_traceroutes}>
						Отключить трассировки</label
					><label class="check"
						><input type="checkbox" bind:checked={form.cdn_unblocked}>
						CDN доступен</label
					>
				</div>
				<div class="flex gap-3">
					<button
						type="submit"
						class="btn-primary flex items-center gap-2"
						disabled={busy !== ""}
					>
						<Plus size={16} />{editing === null ? "Создать" : "Сохранить"}
					</button>
					{#if editing !== null}
						<button
							type="button"
							class="btn"
							onclick={() => { editing = null; form = emptyForm(); }}
						>
							Отмена
						</button>
					{/if}
				</div>
			</form>
		</section>

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
					disabled={selected === null || busy !== ""}
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
					TCP-соединение к хосту на порту 443, ClientHello с указанным SNI,
					затем пакеты с возрастающим TTL. Хост разрешается сканером;
					используется первый IP адрес.
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
					disabled={selected === null || !target.trim() || (traceType === "sni_traceroute" && !sni.trim()) || !Number.isInteger(maxHops) || maxHops < 1 || maxHops > 64 || busy !== ""}
					onclick={() => selected !== null && void command(selected, traceType)}
				>
					<Route size={16} />
					Запустить
				</button>
			</div>
			{#if busy.endsWith(":command")}
				<p class="mt-5 text-sm text-cyan-300">Ожидание ответа сканера…</p>
			{/if}
			{#if result?.type === "error"}
				<p class="mt-5 text-sm text-red-300">{result.message}</p>
			{:else if result?.type === "resubscribe_tasks"}
				<p class="mt-5 text-sm text-emerald-300">
					Запрос на переподписку получен сканером.
				</p>
			{:else if result?.type === "remeasure_dpi_hop"}
				<p class="mt-5 text-sm text-emerald-300">
					DPI hop перемерен: v4 {result.dpi_hop_v4 ?? "—"}; v6
					{result.dpi_hop_v6 ?? "—"}.
				</p>
				<div class="mt-4 grid gap-4 sm:grid-cols-2">
					{#each [{ label: "IPv4", hops: result.dpi_hops_v4 }, { label: "IPv6", hops: result.dpi_hops_v6 }] as trace}
						<div>
							<h3 class="mb-2 font-semibold">{trace.label}</h3>
							{@render dpiHops(trace.hops)}
						</div>
					{/each}
				</div>
			{:else if result?.type === "sni_traceroute"}
				<div class="mt-6">
					<h3 class="mb-2 font-semibold">
						SNI маршрут до {result.host} ({result.target})
					</h3>
					<p class="mb-4 text-sm text-neutral-400">
						SNI: {result.sni}; DPI hop: {result.dpi_hop ?? "—"}
					</p>
					{@render dpiHops(result.hops)}
				</div>
			{:else if result?.type === "traceroute"}
				<div class="mt-6">
					<h3 class="mb-4 font-semibold">Маршрут до {result.target}</h3>
					<ol class="space-y-1">
						{#each result.hops as hop}
							<li
								class="grid grid-cols-[2.5rem_1fr_auto] gap-3 rounded-lg border border-neutral-800 bg-black/20 px-3 py-2 text-sm"
								class:!border-emerald-800={hop.outcome === "connected" || hop.outcome === "rst"}
							>
								<span class="font-mono text-neutral-500">{hop.ttl}</span>
								<div>
									<span class="font-mono" class:text-neutral-500={!hop.address}
										>{hop.address ?? "* * *"}</span
									>
									{#if hop.reverse_names.length}
										<span class="ml-3 text-xs text-neutral-400"
											>{hop.reverse_names.join(", ")}</span
										>
									{/if}
								</div>
								<span class="text-xs text-neutral-500"
									>{outcomeLabel[hop.outcome] ?? hop.outcome}</span
								>
							</li>
						{/each}
					</ol>
					{#if result.hops.length === 0}
						<p class="text-sm text-neutral-500">Ответов нет.</p>
					{/if}
				</div>
			{/if}
		</section>
	{/if}
</div>

<style>
.panel {
	border: 1px solid #303030;
	border-radius: 14px;
	background: #171717;
	padding: 1.5rem;
}
.input {
	display: block;
	width: 100%;
	margin-top: 0.4rem;
	border: 1px solid #404040;
	border-radius: 8px;
	background: #0a0a0a;
	padding: 0.65rem 0.75rem;
	color: #eee;
	outline: none;
}
.input:focus {
	border-color: #22d3ee;
}
.field {
	display: block;
	font-size: 0.8rem;
	color: #aaa;
}
.check {
	display: flex;
	align-items: center;
	gap: 0.5rem;
	color: #d4d4d4;
}
.check input {
	accent-color: #22d3ee;
}
.btn,
.btn-primary {
	border-radius: 8px;
	padding: 0.65rem 0.9rem;
	font-size: 0.85rem;
	cursor: pointer;
}
.btn {
	border: 1px solid #404040;
	background: #262626;
	color: #e5e5e5;
}
.btn-primary {
	border: 1px solid #0891b2;
	background: #0891b2;
	color: white;
	font-weight: 600;
}
button:disabled {
	opacity: 0.45;
	cursor: not-allowed;
}
.link {
	display: inline-flex;
	align-items: center;
	gap: 0.4rem;
	color: #67e8f9;
	font-size: 0.8rem;
	cursor: pointer;
}
.link:hover {
	text-decoration: underline;
}
.link.remove-link {
	color: #f87171;
}
.flag {
	border: 1px solid #404040;
	border-radius: 5px;
	padding: 0.15rem 0.4rem;
	color: #a3a3a3;
	font-size: 0.7rem;
	cursor: pointer;
}
.flag.active {
	border-color: #0e7490;
	color: #67e8f9;
}
</style>
