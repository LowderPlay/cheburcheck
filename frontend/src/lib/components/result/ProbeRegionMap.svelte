<script lang="ts">
import { tick } from "svelte";
import type { DisplayProbeVerdict, ProbeResult } from "$lib/api/probe";
import {
	voteCount as countVotes,
	regionName,
	verdictOrder,
	verdicts,
} from "./probePresentation";
import { regionCode, russiaSubjects, subjectName } from "./russiaRegions";
import { russiaTilePositions } from "./russiaTilePositions";

let {
	probes,
	isStaticCdn,
	selectedSubjectId,
	onSelectRegion,
}: {
	probes: ProbeResult[];
	isStaticCdn: boolean;
	selectedSubjectId: string | null;
	onSelectRegion: (id: string) => void;
} = $props();
const voteCount = (members: ProbeResult[], verdict: DisplayProbeVerdict) =>
	countVotes(members, verdict, isStaticCdn);

const mapColors = {
	blocked: "#ef4444",
	mixed: "mixed",
	available: "#22c55e",
	whitelist: "#f59e0b",
	uncertain: "#737373",
	noResults: "#262626",
} as const;
const mapLegend = [
	{ label: "Блокировка", color: mapColors.blocked },
	{ label: "Доступно", color: mapColors.available },
	{ label: "Исключение", color: mapColors.whitelist },
	{ label: "Неясно", color: mapColors.uncertain },
	{ label: "Нет ответов", color: mapColors.noResults },
];
const regionColor = (members: ProbeResult[] | undefined) => {
	if (!members?.length) return mapColors.noResults;
	const blocked = (
		["tspu_block", "sni_block", "dns_spoofing", "cdn_block"] as const
	).some((verdict) => voteCount(members, verdict) > 0);
	const available = voteCount(members, "ok") > 0;
	if (blocked && available) return mapColors.mixed;
	if (blocked) return mapColors.blocked;
	if (voteCount(members, "whitelist")) return mapColors.whitelist;
	return available ? mapColors.available : mapColors.uncertain;
};
const mixedRegionColors = (members: ProbeResult[]) => [
	mapColors.blocked,
	...(voteCount(members, "whitelist") ? [mapColors.whitelist] : []),
	mapColors.available,
	...(voteCount(members, "uncertain") ? [mapColors.uncertain] : []),
];
const regions = $derived.by(() => {
	const groups = new Map<string, ProbeResult[]>();
	for (const probe of probes) {
		const name = regionName(probe);
		if (!groups.has(name)) groups.set(name, []);
		groups.get(name)?.push(probe);
	}
	return [...groups]
		.map(([name, members]) => ({ name, members }))
		.sort(
			(a, b) =>
				b.members.length - a.members.length ||
				a.name.localeCompare(b.name, "ru"),
		);
});
const regionResults = $derived.by(() => {
	const groups = new Map<string, ProbeResult[]>();
	for (const probe of probes) {
		const code = regionCode(probe.region);
		if (!code) continue;
		if (!groups.has(code)) groups.set(code, []);
		groups.get(code)?.push(probe);
	}
	return groups;
});
const visibleMapLegend = $derived.by(() => {
	const colors = new Set(
		russiaSubjects
			.filter(
				(subject) =>
					mapView === "geographic" || russiaTilePositions[subject.id],
			)
			.flatMap((subject) => {
				const members = regionResults.get(subject.id);
				const color = regionColor(members);
				return color === mapColors.mixed
					? mixedRegionColors(members ?? [])
					: [color];
			}),
	);
	return mapLegend.filter(({ color }) => colors.has(color));
});
const mappedRegions = $derived(
	russiaSubjects
		.filter((subject) => regionResults.has(subject.id))
		.map((subject) => ({
			id: subject.id,
			name: subjectName(subject.id),
			members: regionResults.get(subject.id) ?? [],
		}))
		.sort(
			(a, b) =>
				b.members.length - a.members.length ||
				a.name.localeCompare(b.name, "ru"),
		),
);
const unmappedRegions = $derived(
	regions.filter((region) => !regionCode(region.name)),
);
let mapElement = $state<HTMLDivElement>();
let mapView = $state<"geographic" | "tiles">("geographic");
let activeSubjectId = $state<string | null>(null);
let tooltipX = $state(0);
let tooltipY = $state(0);
const activeSubject = $derived(
	russiaSubjects.find((subject) => subject.id === activeSubjectId),
);
const selectedSubject = $derived(
	russiaSubjects.find((subject) => subject.id === selectedSubjectId),
);
const activeTile = $derived(
	activeSubjectId ? russiaTilePositions[activeSubjectId] : undefined,
);
const selectedTile = $derived(
	selectedSubjectId ? russiaTilePositions[selectedSubjectId] : undefined,
);
const activeMembers = $derived(
	activeSubjectId ? regionResults.get(activeSubjectId) : undefined,
);
function positionTooltip(clientX: number, clientY: number) {
	if (!mapElement) return;
	const bounds = mapElement.getBoundingClientRect();
	const halfWidth = Math.min(128, bounds.width / 2);
	tooltipX = Math.max(
		halfWidth,
		Math.min(bounds.width - halfWidth, clientX - bounds.left),
	);
	tooltipY = Math.max(80, clientY - bounds.top - 10);
}

function selectMapView(view: "geographic" | "tiles") {
	mapView = view;
	activeSubjectId = null;
}

function handleMapTabKeydown(event: KeyboardEvent) {
	if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
	event.preventDefault();
	const next = mapView === "geographic" ? "tiles" : "geographic";
	selectMapView(next);
	const tabId =
		next === "geographic" ? "probe-map-geographic-tab" : "probe-map-tiles-tab";
	document.getElementById(tabId)?.focus();
}

function showSubject(event: PointerEvent, id: string) {
	activeSubjectId = id;
	positionTooltip(event.clientX, event.clientY);
}

function focusSubject(event: Event, id: string) {
	const bounds = (event.currentTarget as Element).getBoundingClientRect();
	activeSubjectId = id;
	positionTooltip(bounds.left + bounds.width / 2, bounds.top);
}

async function selectSubject(id: string) {
	onSelectRegion(id);
	activeSubjectId = null;
	await tick();
	document
		.getElementById("probe-region-details")
		?.scrollIntoView({ block: "nearest" });
}

function handleSubjectKeydown(event: KeyboardEvent, id: string) {
	if (event.key === "Escape") {
		activeSubjectId = null;
		return;
	}
	if (event.key === "Enter" || event.key === " ") {
		event.preventDefault();
		selectSubject(id);
	}
}
</script>

<section
	class="order-2 min-w-0 rounded-lg border border-neutral-800 bg-neutral-900/10 p-4 lg:order-none lg:col-start-1 lg:row-start-1"
	aria-labelledby="probe-regions"
>
	<div class="mb-2 flex items-baseline justify-between gap-2">
		<h4 id="probe-regions" class="font-semibold text-neutral-100">
			По регионам
		</h4>
		<span class="text-xs text-neutral-500"
			>{mapView === "geographic" ? mappedRegions.length : mappedRegions.filter(({ id }) => russiaTilePositions[id]).length}
			регионов · {probes.length} сканеров</span
		>
	</div>
	<div
		class="mb-3 flex gap-1 border-b border-neutral-800"
		role="tablist"
		aria-label="Вид карты"
	>
		<button
			id="probe-map-geographic-tab"
			type="button"
			role="tab"
			aria-selected={mapView === "geographic"}
			aria-controls="probe-map-panel"
			tabindex={mapView === "geographic" ? 0 : -1}
			class={`border-b-2 px-3 py-1.5 text-xs font-medium ${mapView === "geographic" ? "border-primary text-neutral-100" : "border-transparent text-neutral-400 hover:text-neutral-200"}`}
			onclick={() => selectMapView("geographic")}
			onkeydown={handleMapTabKeydown}
		>
			Обычная карта
		</button>
		<button
			id="probe-map-tiles-tab"
			type="button"
			role="tab"
			aria-selected={mapView === "tiles"}
			aria-controls="probe-map-panel"
			tabindex={mapView === "tiles" ? 0 : -1}
			class={`border-b-2 px-3 py-1.5 text-xs font-medium ${mapView === "tiles" ? "border-primary text-neutral-100" : "border-transparent text-neutral-400 hover:text-neutral-200"}`}
			onclick={() => selectMapView("tiles")}
			onkeydown={handleMapTabKeydown}
		>
			Плиточная карта
		</button>
	</div>
	<div
		id="probe-map-panel"
		role="tabpanel"
		aria-labelledby={mapView === "geographic" ? "probe-map-geographic-tab" : "probe-map-tiles-tab"}
		bind:this={mapElement}
		class="relative rounded-md border border-neutral-800 bg-neutral-950/40 px-2 py-2"
	>
		{#if mapView === "geographic"}
			<svg
				class="block h-[220px] w-full sm:h-[320px] lg:h-[340px]"
				viewBox="0 0 520 250"
				role="group"
				aria-label="Карта регионов с результатами динамической проверки"
			>
				<defs>
					{#each russiaSubjects as subject (subject.id)}
						{@const members = regionResults.get(subject.id)}
						{#if regionColor(members) === mapColors.mixed}
							{@const colors = mixedRegionColors(members ?? [])}
							<pattern
								id={`probe-map-mixed-${subject.id}`}
								patternUnits="userSpaceOnUse"
								width={colors.length * 5}
								height="5"
								patternTransform="rotate(45)"
							>
								{#each colors as color, index}
									<rect x={index * 5} y="0" width="5" height="5" fill={color} />
								{/each}
							</pattern>
						{/if}
					{/each}
				</defs>
				{#each russiaSubjects as subject (subject.id)}
					{@const members = regionResults.get(subject.id)}
					<path
						d={subject.path}
						fill={regionColor(members) === mapColors.mixed ? `url(#probe-map-mixed-${subject.id})` : regionColor(members)}
						stroke="#525252"
						stroke-width="0.8"
						stroke-linejoin="round"
						class="cursor-pointer focus:outline-none"
						role="button"
						tabindex="0"
						aria-label={subjectName(subject.id)}
						aria-expanded={selectedSubjectId === subject.id}
						aria-controls="probe-region-details"
						aria-describedby={activeSubjectId === subject.id ? "probe-map-tooltip" : undefined}
						onclick={() => selectSubject(subject.id)}
						onpointerenter={(event) => showSubject(event, subject.id)}
						onpointermove={(event) => showSubject(event, subject.id)}
						onpointerdown={(event) => showSubject(event, subject.id)}
						onpointerleave={(event) => { if (event.pointerType !== "touch") activeSubjectId = null; }}
						onfocus={(event) => focusSubject(event, subject.id)}
						onblur={() => activeSubjectId = null}
						onkeydown={(event) => handleSubjectKeydown(event, subject.id)}
					></path>
				{/each}
				{#if activeSubject}
					<path
						d={activeSubject.path}
						fill="none"
						stroke="white"
						stroke-width="1.8"
						stroke-linejoin="round"
						class="pointer-events-none"
						aria-hidden="true"
					></path>
				{/if}
				{#if selectedSubject}
					<path
						d={selectedSubject.path}
						fill="none"
						stroke="#fafafa"
						stroke-width="1.8"
						stroke-linejoin="round"
						class="pointer-events-none"
						aria-hidden="true"
					></path>
				{/if}
			</svg>
		{:else}
			<div class="h-[300px] overflow-x-auto sm:h-[340px] lg:h-[360px]">
				<svg
					class="block h-full min-w-[550px] w-full"
					viewBox="0 0 550 330"
					role="group"
					aria-label="Плиточная карта регионов с результатами динамической проверки"
				>
					<defs>
						{#each russiaSubjects as subject (subject.id)}
							{@const members = regionResults.get(subject.id)}
							{#if regionColor(members) === mapColors.mixed}
								{@const colors = mixedRegionColors(members ?? [])}
								<pattern
									id={`probe-map-mixed-${subject.id}`}
									patternUnits="userSpaceOnUse"
									width={colors.length * 5}
									height="5"
									patternTransform="rotate(45)"
								>
									{#each colors as color, index}
										<rect
											x={index * 5}
											y="0"
											width="5"
											height="5"
											fill={color}
										/>
									{/each}
								</pattern>
							{/if}
						{/each}
					</defs>
					{#each russiaSubjects as subject (subject.id)}
						{@const tile = russiaTilePositions[subject.id]}
						{@const members = regionResults.get(subject.id)}
						{@const color = regionColor(members)}
						{#if tile}
							<g
								transform={`translate(${tile[0] * 28 + 8} ${tile[1] * 28 + 10})`}
								role="button"
								tabindex="0"
								aria-label={subjectName(subject.id)}
								aria-expanded={selectedSubjectId === subject.id}
								aria-controls="probe-region-details"
								aria-describedby={activeSubjectId === subject.id ? "probe-map-tooltip" : undefined}
								onclick={() => selectSubject(subject.id)}
								onpointerenter={(event) => showSubject(event, subject.id)}
								onpointermove={(event) => showSubject(event, subject.id)}
								onpointerdown={(event) => showSubject(event, subject.id)}
								onpointerleave={(event) => { if (event.pointerType !== "touch") activeSubjectId = null; }}
								onfocus={(event) => focusSubject(event, subject.id)}
								onblur={() => activeSubjectId = null}
								onkeydown={(event) => handleSubjectKeydown(event, subject.id)}
							>
								<rect
									width="27"
									height="27"
									rx="2"
									fill={color === mapColors.mixed ? `url(#probe-map-mixed-${subject.id})` : color}
									stroke="#525252"
									stroke-width="0.7"
								/>
								<text
									x="13.5"
									y="13.5"
									text-anchor="middle"
									dominant-baseline="central"
									font-size="9.5"
									font-weight="600"
									fill={color === mapColors.noResults ? "#fafafa" : "#0a0a0a"}
									class="pointer-events-none select-none"
								>
									{tile[2]}
								</text>
							</g>
						{/if}
					{/each}
					{#if activeTile}
						<rect
							x={activeTile[0] * 28 + 8}
							y={activeTile[1] * 28 + 10}
							width="27"
							height="27"
							rx="2"
							fill="none"
							stroke="#fafafa"
							stroke-width="1.5"
							class="pointer-events-none"
							aria-hidden="true"
						/>
					{/if}
					{#if selectedTile}
						<rect
							x={selectedTile[0] * 28 + 8}
							y={selectedTile[1] * 28 + 10}
							width="27"
							height="27"
							rx="2"
							fill="none"
							stroke="#fafafa"
							stroke-width="1.8"
							class="pointer-events-none"
							aria-hidden="true"
						/>
					{/if}
				</svg>
			</div>
		{/if}
		{#if activeSubject}
			<div
				id="probe-map-tooltip"
				class="pointer-events-none absolute z-10 w-64 rounded-md border border-neutral-600 bg-neutral-950/95 p-3 text-xs shadow-xl"
				style:left={`${tooltipX}px`}
				style:top={`${tooltipY}px`}
				style:transform="translate(-50%, -100%)"
				role="tooltip"
			>
				<p class="font-semibold text-neutral-100">
					{subjectName(activeSubject.id)}
				</p>
				{#if activeMembers?.length}
					<p class="mt-1 text-neutral-400">
						Ответили {activeMembers.length} сканеров
					</p>
					<div class="mt-2 flex flex-wrap gap-x-2 gap-y-1">
						{#each verdictOrder as verdict}
							{@const count = voteCount(activeMembers, verdict)}
							{#if count}
								<span class={verdicts[verdict].color}
									>{verdicts[verdict].label}: {count}</span
								>
							{/if}
						{/each}
					</div>
				{:else}
					<p class="mt-1 text-neutral-500">Нет ответов сканеров</p>
				{/if}
			</div>
		{/if}
	</div>
	{#if unmappedRegions.length}
		<p class="mt-2 text-xs text-neutral-500">
			Нет границ в наборе данных:
			{unmappedRegions.map((region) => region.name).join(', ')}.
		</p>
	{/if}
	<div
		class="mt-2 flex flex-wrap gap-x-4 gap-y-1.5 text-[11px] text-neutral-300"
		aria-label="Легенда карты"
	>
		{#each visibleMapLegend as item (item.label)}
			<span class="inline-flex items-center gap-1.5">
				<span
					class="h-2.5 w-2.5 shrink-0 rounded-sm border border-neutral-500/40"
					style:background-color={item.color}
					aria-hidden="true"
				></span>
				{item.label}
			</span>
		{/each}
	</div>
	<p class="mt-2 text-[11px] text-neutral-500">
		Наведите на регион для краткой сводки. Нажмите на него, чтобы увидеть
		результаты всех сканеров.
	</p>
	<p class="mt-1 text-[10px] text-neutral-600">
		Данные карты:
		<a
			class="underline hover:text-neutral-400"
			href="https://www.geoboundaries.org/"
			>geoBoundaries</a
		>,
		<a
			class="underline hover:text-neutral-400"
			href="https://www.openstreetmap.org/copyright"
			>© OpenStreetMap contributors</a
		>.
	</p>
</section>
