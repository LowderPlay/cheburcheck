<script lang="ts">
import { outcomeLabel } from "$lib/admin/probes";
import type { CommandResult } from "$lib/api/adminModels";
import CalibrationDebugView from "./CalibrationDebugView.svelte";
import DpiHops from "./DpiHops.svelte";
import TcpDiagnosticsView from "./TcpDiagnosticsView.svelte";

let { result }: { result: CommandResult | null } = $props();
</script>

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
		{#each [{ label: "IPv4", hops: result.dpi_hops_v4, calibration: result.calibration_v4 }, { label: "IPv6", hops: result.dpi_hops_v6, calibration: result.calibration_v6 }] as trace}
			<div>
				<h3 class="mb-2 font-semibold">{trace.label}</h3>
				<CalibrationDebugView calibration={trace.calibration} />
				<DpiHops hops={trace.hops} />
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
		<DpiHops hops={result.hops} />
		<CalibrationDebugView calibration={result.calibration} />
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
					{#if hop.tcp_diagnostics}
						<div class="col-span-3 min-w-0">
							<TcpDiagnosticsView diagnostics={hop.tcp_diagnostics} />
						</div>
					{/if}
				</li>
			{/each}
		</ol>
		{#if result.hops.length === 0}
			<p class="text-sm text-neutral-500">Ответов нет.</p>
		{/if}
	</div>
{/if}
