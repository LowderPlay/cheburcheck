<script lang="ts">
import { LoaderCircle } from "@lucide/svelte";
import { selectProbeVerdict, type ProbeStatus } from "$lib/api/probe";
import { fetchSharedResult, type SharedResult } from "$lib/api/share";
import ResultPanel from "$lib/components/ResultPanel.svelte";
import ProbeTable from "$lib/components/result/ProbeTable.svelte";
import type { PageProps } from "./$types";

let { data }: PageProps = $props();
let shared = $state<SharedResult | null>(null);
let loading = $state(true);
let loadError = $state("");

$effect(() => {
	const controller = new AbortController();
	const id = data.id;
	shared = null;
	loading = true;
	loadError = "";
	fetchSharedResult(id, fetch, controller.signal)
		.then((result) => (shared = result))
		.catch((cause: unknown) => {
			if (controller.signal.aborted) return;
			loadError = cause instanceof Error ? cause.message : "Не удалось загрузить результат";
		})
		.finally(() => {
			if (!controller.signal.aborted) loading = false;
		});
	return () => controller.abort();
});

const snapshot = $derived(shared?.snapshot);
const result = $derived(snapshot?.result);
const probes = $derived(snapshot?.probes ?? []);
const probeStatus = $derived<ProbeStatus | null>(
	snapshot?.probe_status
		? {
				id: shared?.id ?? "",
				target: result?.target ?? "",
				...snapshot.probe_status,
				status: snapshot.probe_status.status,
			}
		: null,
);
const probeVerdict = $derived(
	selectProbeVerdict(probes, result ? result.providers.length > 0 : false),
);
const probingAtShareTime = $derived(
	snapshot?.probe_status?.status === "started" ||
		snapshot?.probe_status?.status === "progress",
);
const createdAt = $derived(
	shared
		? new Intl.DateTimeFormat("ru-RU", {
				dateStyle: "medium",
				timeStyle: "short",
			}).format(new Date(shared.created_at))
		: "",
);
</script>

<svelte:head>
	<title>
		{result
			? `Результат проверки ${result.target} - Cheburcheck`
			: "Общий результат - Cheburcheck"}
	</title>
	<meta name="robots" content="noindex, nofollow">
	<meta
		name="description"
		content={result
			? `Сохранённый результат проверки ${result.target} в Cheburcheck`
			: "Сохранённый результат проверки в Cheburcheck"}
	>
</svelte:head>

{#if loading}
	<LoaderCircle
		class="mx-auto mt-8 animate-spin text-neutral-400"
		aria-label="Загрузка результата"
	/>
{:else if loadError}
	<div class="mt-4 border border-red-900/30 bg-red-950/10 p-6 text-sm text-red-400" role="alert">
		{loadError}
	</div>
{:else if shared && result}
	<div class="mb-6 border border-neutral-800 bg-neutral-900/20 p-4 text-sm text-neutral-400">
		<p>
			Результат проверки сформирован сервером при создании ссылки
			<time datetime={shared.created_at}>{createdAt}</time>.
			Статические данные проверены заново; блок динамической проверки содержит ответы сохранённого запуска.
		</p>
		<p class="mt-1 text-xs text-neutral-500">
			Ссылка действует до
			<time datetime={shared.expires_at}>
				{new Intl.DateTimeFormat("ru-RU", {
					dateStyle: "medium",
					timeStyle: "short",
				}).format(new Date(shared.expires_at))}
			</time>.
		</p>
	</div>

	<ResultPanel {result} probeVerdict={probeVerdict}>
		{#if probeStatus && probeStatus.online_probes > 0 && (probes.length > 0 || !probingAtShareTime)}
			<ProbeTable
				{probes}
				status={probeStatus}
				isStaticCdn={result.providers.length > 0}
			/>
		{:else if probeStatus && probingAtShareTime}
			<section class="mt-8" aria-label="Результаты динамической проверки">
				<h3 class="border-b border-neutral-800 pb-2 text-sm font-bold text-white uppercase">
					Результаты динамической проверки
				</h3>
				<p class="mt-3 text-sm text-neutral-400">
					На момент создания ссылки сканеры ещё выполняли проверку. Снимок содержит только уже полученные ответы.
				</p>
			</section>
		{:else if probeStatus || probes.length > 0}
			<section class="mt-8" aria-label="Результаты динамической проверки">
				<h3 class="border-b border-neutral-800 pb-2 text-sm font-bold text-white uppercase">
					Результаты динамической проверки
				</h3>
				<p class="mt-3 text-sm text-neutral-400">
					{#if snapshot?.probe_status?.status === "error"}
						Соединение с динамической проверкой прервалось. Получено ответов: {probes.length}.
					{:else}
					Динамическая проверка завершена. Получено ответов: {probes.length}.
				{/if}
				</p>
			</section>
		{/if}
	</ResultPanel>
{/if}
