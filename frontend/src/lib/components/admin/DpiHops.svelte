<script lang="ts">
import { outcomeLabel } from "$lib/admin/probes";
import type { DpiHop } from "$lib/api/adminModels";
import TcpDiagnosticsView from "./TcpDiagnosticsView.svelte";

let { hops }: { hops: DpiHop[] } = $props();
</script>

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
			{#if hop.tcp_diagnostics}
				<div class="col-span-3 min-w-0">
					<TcpDiagnosticsView diagnostics={hop.tcp_diagnostics} />
				</div>
			{/if}
		</li>
	{/each}
</ol>
{#if hops.length === 0}
	<p class="text-sm text-neutral-500">
		Измерение не удалось: ответов нет. Подробности в логах сканера.
	</p>
{/if}
