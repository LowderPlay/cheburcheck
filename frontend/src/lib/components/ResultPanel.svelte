<script lang="ts">
import {
	Activity,
	EthernetPort,
	Globe,
	Hash,
	Info,
	Layers,
	MapPin,
	Network,
	ScrollText,
	Server,
	ShieldCheck,
} from "@lucide/svelte";
import type { CheckResult } from "$lib/api/check";
import DetailRow from "./DetailRow.svelte";
import ResultIpList from "./result/ResultIpList.svelte";
import ResultStatusHeader from "./result/ResultStatusHeader.svelte";
import ResultStringList from "./result/ResultStringList.svelte";
import ResultTargetCard from "./result/ResultTargetCard.svelte";

type Provider = { name: string; networks: { cidr: string }[] };
type ResultTheme = "blocked" | "clean" | "whitelist";

let {
	result,
}: {
	result: CheckResult;
} = $props();

const valueClass = "text-right text-sm font-medium text-neutral-200";
const alertValueClass = `${valueClass} text-red-500`;
const successValueClass = `${valueClass} text-green-500`;

const theme = $derived<ResultTheme>(
	result.whitelist && !result.domain
		? "whitelist"
		: result.found
			? "blocked"
			: "clean",
);
const panelClass = $derived(
	theme === "whitelist"
		? "border-yellow-900/30 bg-[#2e2d05]/10"
		: theme === "blocked"
			? "border-red-900/30 bg-[#450a0a]/10"
			: "border-green-900/30 bg-[#052e16]/10",
);
const reasonHeaderClass = $derived(
	theme === "whitelist"
		? "border-yellow-900/30 text-[#f0b100]"
		: theme === "blocked"
			? "border-red-900/30 text-red-400"
			: "border-green-900/30 text-green-500",
);
const allPrefixes = $derived(result.asnInfo?.prefixes ?? []);
const blockedPrefixes = $derived(result.asnInfo?.blockedPrefixes ?? []);
const whitelistDate = $derived(
	result.whitelist?.lastOk
		? new Date(result.whitelist.lastOk).toLocaleDateString("ru-RU")
		: "-",
);
const providerCidrs = (provider: Provider) =>
	provider.networks.map((network) => network.cidr);
</script>

<div class="mt-8 space-y-6">
	<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
		<div class={`border p-4 rounded-lg flex items-center ${panelClass}`}>
			<ResultStatusHeader {theme} blocked={result.blocked} />
		</div>
		<ResultTargetCard
			targetType={result.targetType}
			target={result.target}
			asnStats={result.asnInfo ? { total: allPrefixes.length, blocked: blockedPrefixes.length } : undefined}
		/>
	</div>

	<div class="grid grid-cols-1 gap-6 md:grid-cols-2">
		<div class="space-y-4">
			<div class="flex items-center gap-2 border-b border-neutral-800 pb-2">
				<h3
					class="text-sm font-bold text-white uppercase flex items-center gap-2"
				>
					<Network size={16} class="text-primary" />
					Сетевые данные
				</h3>
			</div>

			<div
				class="border border-neutral-800 rounded-lg bg-neutral-900/10 px-4 py-1"
			>
				{#if !result.asnInfo}
					<DetailRow label="IP-адреса" icon={Globe}>
						<ResultIpList ips={result.ips} subnetSize={result.subnetSize} />
					</DetailRow>
				{/if}

				{#if result.reverseLookup.length > 0}
					<DetailRow label="Обратный DNS" icon={EthernetPort}>
						<div class="flex w-full flex-col items-end gap-2">
							{#each result.reverseLookup as ptr}
								<span class={valueClass}>
									<a
										href={`/check?target=${ptr}`}
										class="text-neutral-100 underline decoration-neutral-500 transition-all hover:text-white hover:decoration-neutral-100"
									>
										{ptr}
									</a>
								</span>
							{/each}
						</div>
					</DetailRow>
				{/if}

				<DetailRow label="Хостинг / ISP" icon={Server}>
					<span class={valueClass}>{result.geo.organisation || "-"}</span>
				</DetailRow>
				<DetailRow label="Локация" icon={MapPin}>
					<span class={valueClass}>{result.geo.location}</span>
				</DetailRow>
				<DetailRow label="ASN" icon={Hash}>
					<span class={valueClass}>
						{#if result.geo.asn}
							<a
								href={`/check?target=${result.geo.asn}`}
								class="text-neutral-100 underline decoration-neutral-500 transition-all hover:text-white hover:decoration-neutral-100"
							>
								{result.geo.asn}
							</a>
						{:else}
							-
						{/if}
					</span>
				</DetailRow>

				{#if result.asnInfo}
					<DetailRow label="Подсети ASN" icon={Layers}>
						<ResultStringList items={allPrefixes} />
					</DetailRow>
				{/if}
			</div>
		</div>

		<div class="space-y-4">
			<div class="flex items-center gap-2 border-b border-neutral-800 pb-2">
				<h3
					class="text-sm font-bold text-white uppercase flex items-center gap-2"
				>
					<ScrollText size={16} class="text-primary" />
					Нахождение в списках
				</h3>
			</div>

			<div
				class="border border-neutral-800 rounded-lg bg-neutral-900/10 px-4 py-1"
			>
				<DetailRow label="CDN" icon={Activity}>
					{#if result.providers.length > 0}
						<p class={alertValueClass}>НАЙДЕН</p>
					{:else}
						<span class={valueClass}>Не найден</span>
					{/if}
				</DetailRow>

				{#each result.providers as provider}
					<DetailRow label={provider.name} icon={Server}>
						<ResultStringList items={providerCidrs(provider)} limit={5} />
					</DetailRow>
				{/each}

				{#if result.whitelist}
					<DetailRow
						label="Белый список (?)"
						href="/kb/whitelist"
						icon={ShieldCheck}
					>
						<span class={successValueClass}>
							НАЙДЕН -
							<span
								class="underline decoration-dotted"
								title="Дата последнего сканирования, когда данный домен был найден в белом списке"
							>
								{whitelistDate}
							</span>
						</span>
					</DetailRow>
				{/if}

				<DetailRow label="Реестр РКН" icon={ScrollText}>
					{#if result.domain}
						<span class={alertValueClass}>ОГРАНИЧЕН</span>
					{:else if result.blockedSubnets.length > 0}
						<span
							class={`${alertValueClass} underline decoration-dotted`}
							title="Адреса пересекаются с подсетями заблокированных доменов (не гарантирует блокировку)"
						>
							IP-АДРЕСА
						</span>
					{:else}
						<span class={valueClass}>Не найден</span>
					{/if}
				</DetailRow>

				{#if result.domain}
					<DetailRow label="Заблокированный домен" icon={Info}>
						<span class={valueClass}>{result.domain}</span>
					</DetailRow>
				{/if}

				{#if result.blockedSubnets.length > 0 && !result.asnInfo}
					<DetailRow label="Заблокированные подсети" icon={Layers}>
						<div class="text-right">
							{#each result.blockedSubnets as network}
								<p class={valueClass}>{network}</p>
							{/each}
						</div>
					</DetailRow>
				{/if}

				{#if result.asnInfo && blockedPrefixes.length > 0}
					<DetailRow label="Заблокированные подсети ASN" icon={Layers}>
						<ResultStringList items={blockedPrefixes} alert />
					</DetailRow>
				{/if}
			</div>
		</div>
	</div>
</div>
