<script lang="ts">
import type { DisplayProbeVerdict, ProbeResult } from "$lib/api/probe";
import {
	voteCount as countVotes,
	verdictOrder,
	verdicts,
} from "./probePresentation";

let {
	probes,
	isStaticCdn,
	onHighlightVerdict,
}: {
	probes: ProbeResult[];
	isStaticCdn: boolean;
	onHighlightVerdict: (verdict: DisplayProbeVerdict | null) => void;
} = $props();
const voteCount = (members: ProbeResult[], verdict: DisplayProbeVerdict) =>
	countVotes(members, verdict, isStaticCdn);
</script>

<div
	class="order-1 min-w-0 rounded-lg border border-neutral-800 bg-neutral-900/20 p-4 lg:order-none"
>
	<div class="mb-4 flex flex-wrap items-baseline justify-between gap-2">
		<h4 class="font-semibold text-neutral-100">Что обнаружено</h4>
		<p class="text-xs text-neutral-500">
			Число сканеров с каждым вердиктом. У сканера может быть несколько
			вердиктов.
		</p>
	</div>
	<div class="grid gap-2">
		{#each verdictOrder as verdict}
			{@const count = voteCount(probes, verdict)}
			{#if count > 0}
				{@const view = verdicts[verdict]}
				<a
					href={verdict === "whitelist" ? "/kb/whitelist" : "/kb/probing#что-показывает-результат"}
					class="group block rounded-md border border-neutral-800 bg-neutral-950/40 p-3 transition-colors hover:border-neutral-600 hover:bg-neutral-900/60 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
					onpointerenter={() => onHighlightVerdict(verdict)}
					onpointerleave={() => onHighlightVerdict(null)}
					onfocus={() => onHighlightVerdict(verdict)}
					onblur={() => onHighlightVerdict(null)}
				>
					<div class="flex items-center justify-between gap-2 text-sm">
						<span
							class={`flex items-center gap-2 font-medium group-hover:underline ${view.color}`}
							><view.icon size={16} />{view.label}</span
						><strong class="text-neutral-100"
							>{count}
							<span class="font-normal text-neutral-500">
								/ {probes.length}</span
							></strong
						>
					</div>
					<div class="mt-2 h-1.5 overflow-hidden rounded-full bg-neutral-800">
						<div
							class={`h-full rounded-full ${view.bar}`}
							style:width={`${count / probes.length * 100}%`}
						></div>
					</div>
				</a>
			{/if}
		{/each}
	</div>
</div>
