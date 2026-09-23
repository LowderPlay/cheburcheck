<script lang="ts">
import type { ProbeHostResult, ProbeResult } from "$lib/api/probe";

let { probes }: { probes: ProbeResult[] } = $props();

const hostEvidence = [
	{
		type: "Good",
		label: "Доступно",
		color: "text-green-400",
		bar: "bg-green-500",
	},
	{
		type: "ClientHello",
		label: "После ClientHello",
		color: "text-red-400",
		bar: "bg-red-500",
	},
	{
		type: "DataTimeout",
		label: "Таймаут данных",
		color: "text-amber-400",
		bar: "bg-amber-500",
	},
	{
		type: "ConnectionError",
		label: "Ошибка соединения",
		color: "text-neutral-400",
		bar: "bg-neutral-500",
	},
] as const;
const hostTones = {
	Good: { label: "Доступно", color: "bg-green-500", text: "text-green-400" },
	ClientHello: {
		label: "После ClientHello",
		color: "bg-red-500",
		text: "text-red-400",
	},
	DataTimeout: {
		label: "Таймаут данных",
		color: "bg-amber-500",
		text: "text-amber-400",
	},
	ConnectionError: {
		label: "Ошибка соединения",
		color: "bg-neutral-500",
		text: "text-neutral-400",
	},
	Mixed: {
		label: "Разные ответы",
		color: "",
		text: "text-neutral-300",
	},
} as const;
const hostToneOrder = Object.keys(hostTones) as (keyof typeof hostTones)[];

type HostType = ProbeHostResult["probe_evidence"]["type"];
type HostGroup = {
	id: string;
	category: string;
	results: { probe: ProbeResult; result: ProbeHostResult }[];
};
const hostCount = (host: HostGroup, type: HostType) =>
	host.results.filter(({ result }) => result.probe_evidence.type === type)
		.length;
const hostEvidenceColors: Record<HostType, string> = {
	Good: "#22c55e",
	ClientHello: "#ef4444",
	DataTimeout: "#f59e0b",
	ConnectionError: "#737373",
};
const horizontalBands = (colors: string[]) =>
	`linear-gradient(to bottom, ${colors.map((color, index) => `${color} ${(index / colors.length) * 100}% ${((index + 1) / colors.length) * 100}%`).join(", ")})`;
const hostMixedColors = (host: HostGroup) =>
	hostEvidence
		.filter(({ type }) => hostCount(host, type) > 0)
		.map(({ type }) => hostEvidenceColors[type]);
const hostTone = (host: HostGroup): keyof typeof hostTones => {
	const first = host.results[0]?.result.probe_evidence.type;
	return first &&
		host.results.every(({ result }) => result.probe_evidence.type === first)
		? first
		: "Mixed";
};
const hostDescription = (host: HostGroup) =>
	[
		host.id,
		host.category === "Blacklist"
			? "заблокированный диапазон"
			: "доступный диапазон",
		...hostEvidence.map(
			({ type, label }) => `${label}: ${hostCount(host, type)}`,
		),
	].join(" · ");
const hosts = $derived.by(() => {
	const groups = new Map<string, HostGroup>();
	for (const probe of probes)
		for (const result of probe.host_results ?? []) {
			const key = `${result.host}:${result.host_id}`;
			if (!groups.has(key))
				groups.set(key, {
					id: result.host_id,
					category: result.host,
					results: [],
				});
			groups.get(key)?.results.push({ probe, result });
		}
	const ranked = [...groups.values()].map((host) => ({
		host,
		tone: hostTone(host),
	}));
	const toneTotals = new Map<string, number>();
	for (const { host, tone } of ranked) {
		const key = `${host.category}:${tone}`;
		toneTotals.set(key, (toneTotals.get(key) ?? 0) + host.results.length);
	}
	return ranked
		.sort(
			(a, b) =>
				a.host.category.localeCompare(b.host.category) ||
				(toneTotals.get(`${b.host.category}:${b.tone}`) ?? 0) -
					(toneTotals.get(`${a.host.category}:${a.tone}`) ?? 0) ||
				hostToneOrder.indexOf(a.tone) - hostToneOrder.indexOf(b.tone) ||
				b.host.results.length - a.host.results.length ||
				a.host.id.localeCompare(b.host.id),
		)
		.map(({ host }) => host);
});
const hostDividerCount = $derived(
	hosts
		.slice(1)
		.filter((host, index) => host.category !== hosts[index].category).length,
);
const hostGridColumns = $derived(
	hosts
		.map((host, index) =>
			index > 0 && host.category !== hosts[index - 1].category
				? "4px minmax(6px, 1fr)"
				: "minmax(6px, 1fr)",
		)
		.join(" "),
);
const hostEvidenceCounts = $derived(
	hostEvidence
		.map((item) => ({
			...item,
			count: hosts.filter((host) => hostCount(host, item.type) > 0).length,
		}))
		.filter((item) => item.count > 0),
);
let cdnElement = $state<HTMLElement>();
let activeHostKey = $state<string | null>(null);
let hostTooltipX = $state(0);
let hostTooltipY = $state(0);
const activeHost = $derived(
	hosts.find((host) => `${host.category}:${host.id}` === activeHostKey),
);
function positionHostTooltip(clientX: number, clientY: number) {
	if (!cdnElement) return;
	const bounds = cdnElement.getBoundingClientRect();
	const halfWidth = Math.min(128, bounds.width / 2);
	hostTooltipX = Math.max(
		halfWidth,
		Math.min(bounds.width - halfWidth, clientX - bounds.left),
	);
	hostTooltipY = clientY - bounds.top + 12;
}

function showHost(event: PointerEvent, key: string) {
	activeHostKey = key;
	positionHostTooltip(event.clientX, event.clientY);
}

function focusHost(event: Event, key: string) {
	const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect();
	activeHostKey = key;
	positionHostTooltip(bounds.left + bounds.width / 2, bounds.bottom);
}
</script>

{#if hosts.length}
	<section
		bind:this={cdnElement}
		class="relative order-3 min-w-0 rounded-lg border border-neutral-800 bg-neutral-900/10 p-4 lg:order-none"
		aria-labelledby="probe-hosts"
	>
		<div class="mb-2 flex items-baseline justify-between gap-2">
			<h4 id="probe-hosts" class="font-semibold text-neutral-100">
				Контрольные CDN-серверы
			</h4>
			<span class="text-xs text-neutral-500"
				>{hosts.length}
				серверов</span
			>
		</div>
		<div class="overflow-x-auto pb-1">
			<div
				class="grid h-8 gap-0.5"
				style={`grid-template-columns: ${hostGridColumns}; min-width: max(100%, ${hosts.length * 9 + hostDividerCount * 10}px)`}
				role="group"
				aria-label="Результаты по контрольным CDN-серверам"
			>
				{#each hosts as host, index (`${host.category}:${host.id}`)}
					{@const key = `${host.category}:${host.id}`}
					{@const tone = hostTone(host)}
					{#if index > 0 && hosts[index - 1].category !== host.category}
						<span
							class="block h-full rounded-sm bg-neutral-600"
							aria-hidden="true"
						></span>
					{/if}
					<button
						type="button"
						class={`block h-full min-w-0 rounded-sm focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-white ${hostTones[tone].color}`}
						style:background={tone === "Mixed" ? horizontalBands(hostMixedColors(host)) : undefined}
						aria-label={hostDescription(host)}
						aria-describedby={activeHostKey === key ? "probe-host-tooltip" : undefined}
						onpointerenter={(event) => showHost(event, key)}
						onpointermove={(event) => showHost(event, key)}
						onpointerdown={(event) => showHost(event, key)}
						onpointerleave={(event) => { if (event.pointerType !== "touch") activeHostKey = null; }}
						onfocus={(event) => focusHost(event, key)}
						onblur={() => activeHostKey = null}
						onkeydown={(event) => { if (event.key === "Escape") activeHostKey = null; }}
					></button>
				{/each}
			</div>
		</div>
		{#if activeHost}
			<div
				id="probe-host-tooltip"
				class="pointer-events-none absolute z-10 w-64 rounded-md border border-neutral-600 bg-neutral-950/95 p-3 text-xs shadow-xl"
				style:left={`${hostTooltipX}px`}
				style:top={`${hostTooltipY}px`}
				style:transform="translateX(-50%)"
				role="tooltip"
			>
				<p class="break-all font-semibold text-neutral-100">
					{activeHost.id}
				</p>
				<p class="mt-1 text-neutral-400">
					{activeHost.category === "Blacklist" ? "Заблокированный диапазон" : "Доступный диапазон"}
					· Ответили
					{new Set(activeHost.results.map(({ probe }) => probe.probe_id)).size}
					сканеров
				</p>
				<div class="mt-2 space-y-1">
					{#each hostEvidence as item (item.type)}
						{@const count = hostCount(activeHost, item.type)}
						{#if count}
							<p class={`flex justify-between gap-3 ${item.color}`}>
								<span>{item.label}</span><strong>{count}</strong>
							</p>
						{/if}
					{/each}
				</div>
			</div>
		{/if}
		<p class="mt-1 text-[11px] text-neutral-500">
			Сначала серверы в заблокированных диапазонах ({hosts.filter((host) => host.category === "Blacklist").length}),
			затем в доступных ({hosts.filter((host) => host.category !== "Blacklist").length}).
		</p>
		<div class="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs">
			{#each hostEvidenceCounts as item (item.type)}
				<span class="inline-flex items-center gap-1.5 text-neutral-300">
					<span
						class={`h-2.5 w-2.5 rounded-sm ${item.bar}`}
						aria-hidden="true"
					></span>
					{item.label}: <strong>{item.count}</strong>
				</span>
			{/each}
		</div>
	</section>
{/if}
