<script lang="ts">
import { Activity, LoaderCircle } from "@lucide/svelte";
import type {
	DisplayProbeVerdict,
	ProbeResult,
	ProbeStatus,
} from "$lib/api/probe";
import { scannerWord } from "$lib/utils/russianPlural";
import ProbeDns from "./ProbeDns.svelte";
import ProbeHosts from "./ProbeHosts.svelte";
import ProbeRegionDetails from "./ProbeRegionDetails.svelte";
import ProbeRegionMap from "./ProbeRegionMap.svelte";
import ProbeVerdictSummary from "./ProbeVerdictSummary.svelte";
import { regionCode, subjectName } from "./russiaRegions";

let {
	probes,
	status,
	isStaticCdn,
}: {
	probes: ProbeResult[];
	status: ProbeStatus;
	isStaticCdn: boolean;
} = $props();

let selectedRegionId = $state<string | null>(null);
let highlightedVerdict = $state<DisplayProbeVerdict | null>(null);
const selectedRegionProbes = $derived(
	selectedRegionId
		? probes.filter((probe) => regionCode(probe.region) === selectedRegionId)
		: [],
);
</script>

<section class="mt-8 space-y-5" aria-label="Результаты динамической проверки">
	<div
		class="flex flex-wrap items-center justify-between gap-2 border-b border-neutral-800 pb-2"
	>
		<h3 class="flex items-center gap-2 text-sm font-bold uppercase text-white">
			{#if status.online_probes > 0 && probes.length < status.online_probes && status.status !== "done" && status.status !== "error"}
				<LoaderCircle class="animate-spin text-primary" size={16} />
			{:else}
				<Activity size={16} class="text-primary" />
			{/if}
			<a
				class="underline decoration-dotted underline-offset-2"
				href="/kb/probing"
				>Результаты динамической проверки</a
			>
		</h3>
		<div class="text-xs text-neutral-400 flex items-center gap-2">
			<span
				class={`w-2 h-2 rounded-full ${status.online_probes > 0 ? 'bg-green-500 animate-pulse' : 'bg-neutral-600'}`}
			></span>
			{status.online_probes} {scannerWord(status.online_probes)} в сети,
			получено {probes.length} из {status.online_probes} ответов
		</div>
	</div>
	{#if probes.length === 0}
		<div
			class="rounded-lg border border-neutral-800 bg-neutral-900/20 p-8 text-center text-sm text-neutral-400"
		>
			{#if status.status === "done" || status.status === "error"}
				Сканеры не ответили на запрос или недоступны.
			{:else}
				<LoaderCircle
					class="mx-auto mb-3 animate-spin text-primary"
					size={28}
				/>Ожидание ответов от сканеров...
			{/if}
		</div>
	{:else}
		<div
			class="grid items-start gap-5 lg:grid-cols-[minmax(0,2fr)_minmax(260px,1fr)]"
		>
			<div
				class="contents lg:col-start-2 lg:row-start-1 lg:block lg:min-w-0 lg:space-y-5"
			>
				<ProbeVerdictSummary
					{probes}
					{isStaticCdn}
					onHighlightVerdict={(verdict) => (highlightedVerdict = verdict)}
				/>
				<ProbeHosts {probes} />
			</div>
			<ProbeRegionMap
				{probes}
				{isStaticCdn}
				{highlightedVerdict}
				selectedSubjectId={selectedRegionId}
				onSelectRegion={(id) => (selectedRegionId = id)}
			/>
			{#if selectedRegionId}
				<ProbeRegionDetails
					name={subjectName(selectedRegionId)}
					probes={selectedRegionProbes}
					{isStaticCdn}
					onClose={() => (selectedRegionId = null)}
				/>
			{/if}
		</div>

		<ProbeDns {probes} />
	{/if}
</section>
