<script lang="ts">
import type { DnsObservation, ProbeResult } from "$lib/api/probe";
import { scannersAfterIzWord, scannerWord } from "$lib/utils/russianPlural";

let { probes }: { probes: ProbeResult[] } = $props();
type DnsEntry = { probe: ProbeResult; observation: DnsObservation };

const protocols = ["Udp", "Tcp", "Doh", "Dot"] as const;
const protocolName = { Udp: "UDP", Tcp: "TCP", Doh: "DoH", Dot: "DoT" };
const dnsCounts = (
	entries: DnsEntry[],
	protocol: DnsObservation["protocol"],
) => {
	const selected = entries.filter(
		({ observation }) => observation.protocol === protocol,
	);
	return {
		total: selected.length,
		suspect: selected.filter(
			({ observation }) => observation.suspected_spoofing,
		).length,
		errors: selected.filter(
			({ observation }) => observation.outcome.type === "Error",
		).length,
	};
};
const dnsProviders = $derived.by(() => {
	const groups = new Map<string, DnsEntry[]>();
	for (const probe of probes)
		for (const observation of probe.dns?.observations ?? []) {
			if (!groups.has(observation.provider))
				groups.set(observation.provider, []);
			groups.get(observation.provider)?.push({ probe, observation });
		}
	return [...groups]
		.map(([name, entries]) => ({ name, entries }))
		.sort((a, b) => a.name.localeCompare(b.name));
});
const dnsScanned = $derived(probes.filter((probe) => probe.dns).length);
const dnsConfirmed = $derived(
	probes.filter((probe) => probe.dns?.spoofing_detected).length,
);
</script>

{#if dnsScanned}
	<section
		class="rounded-lg border border-neutral-800 bg-neutral-900/10 p-4"
		aria-labelledby="probe-dns"
	>
		<div class="mb-3 flex flex-wrap items-baseline justify-between gap-2">
			<h4 id="probe-dns" class="font-semibold text-neutral-100">Подмена DNS</h4>
			<span class="text-xs text-neutral-500"
				>Подмена подтверждена у
				{dnsConfirmed}
				из {dnsScanned} {scannersAfterIzWord(dnsScanned)}</span
			>
		</div>
		<div class="overflow-x-auto">
			<table class="w-full min-w-[590px] text-left text-xs">
				<thead class="text-neutral-500">
					<tr class="border-b border-neutral-800">
						<th class="p-2 font-medium">DNS-провайдер</th>
						{#each protocols as protocol}
							<th class="p-2 font-medium">{protocolName[protocol]}</th>
						{/each}
					</tr>
				</thead>
				<tbody>
					{#each dnsProviders as provider (provider.name)}
						{@const providerScannerCount = new Set(provider.entries.map(({ probe }) => probe.probe_id)).size}
						<tr class="border-b border-neutral-800/60">
							<th
								scope="row"
								class="p-2 font-medium capitalize text-neutral-200"
							>
								{provider.name}
								<span class="ml-1 font-normal text-neutral-500"
									>·
									{providerScannerCount}
									{scannerWord(providerScannerCount)}</span
								>
							</th>
							{#each protocols as protocol}
								{@const counts = dnsCounts(provider.entries, protocol)}
								<td class="p-2">
									{#if counts.total}
										{#if counts.suspect}
											<span class="font-semibold text-red-400"
												>{counts.suspect}
												подозр.</span
											>
										{:else}
											<span class="text-green-400">Норма</span>
										{/if}
										<span class="text-neutral-500"> / {counts.total}</span>
										{#if counts.errors}
											<span class="block text-amber-400"
												>Ошибок: {counts.errors}</span
											>
										{/if}
									{:else}
										<span class="text-neutral-600">—</span>
									{/if}
								</td>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</section>
{/if}
