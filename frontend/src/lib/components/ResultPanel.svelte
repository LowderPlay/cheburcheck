<script lang="ts">
import DetailRow from "./DetailRow.svelte";
import Feedback from "./Feedback.svelte";
import ResultAsnSummary from "./result/ResultAsnSummary.svelte";
import ResultIpList from "./result/ResultIpList.svelte";
import ResultStatusHeader from "./result/ResultStatusHeader.svelte";
import ResultStringList from "./result/ResultStringList.svelte";
import ResultTargetCard from "./result/ResultTargetCard.svelte";

type Network = { cidr: string };
type Provider = { name: string; networks: Network[] };
type AsnInfo = { prefixes: string[]; blockedPrefixes: string[] } | null;
type ResultTheme = "blocked" | "clean" | "whitelist";

let {
	result,
}: {
	result: {
		targetType: string;
		target: string;
		id?: string | null;
		found: boolean;
		blocked: boolean;
		whitelist?: { lastOk?: string | null } | null;
		domain?: string | null;
		ips: string[];
		subnetSize?: string | null;
		geo: {
			organisation?: string | null;
			location: string;
			asn?: string | null;
		};
		providers: Provider[];
		blockedSubnets: string[];
		asnInfo: AsnInfo;
	};
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

<div class={`mt-4 border p-6 ${panelClass}`}>
	<ResultStatusHeader {theme} blocked={result.blocked} />
	<ResultTargetCard targetType={result.targetType} target={result.target} />

	{#if result.asnInfo}
		<ResultAsnSummary total={allPrefixes.length} blocked={blockedPrefixes.length} />
	{/if}

	<div class="mb-8 grid grid-cols-1 gap-8 sm:grid-cols-2">
		<div>
			<h3
				class="mb-0 border-b border-neutral-800 pb-2 text-sm font-bold text-white uppercase"
			>
				Сетевые данные
			</h3>

			{#if !result.asnInfo}
				<DetailRow label="IP-адреса">
					<ResultIpList ips={result.ips} subnetSize={result.subnetSize} />
				</DetailRow>
			{/if}

			<DetailRow label="Хостинг / ISP">
				<span class={valueClass}>{result.geo.organisation || "-"}</span>
			</DetailRow>
			<DetailRow label="Локация">
				<span class={valueClass}>{result.geo.location}</span>
			</DetailRow>
			<DetailRow label="ASN">
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
				<DetailRow label="Подсети ASN">
					<ResultStringList items={allPrefixes} />
				</DetailRow>
			{/if}
		</div>

		<div>
			<h3
				class={`mb-0 border-b pb-2 text-sm font-bold uppercase ${reasonHeaderClass}`}
			>
				Нахождение в списках
			</h3>

			<DetailRow label="CDN">
				{#if result.providers.length > 0}
					<p class={alertValueClass}>НАЙДЕН</p>
				{:else}
					<span class={valueClass}>Не найден</span>
				{/if}
			</DetailRow>

			{#each result.providers as provider}
				<DetailRow label={provider.name}>
					<ResultStringList items={providerCidrs(provider)} limit={5} />
				</DetailRow>
			{/each}

			{#if result.whitelist}
				<DetailRow label="Белый список (?)" href="/kb/whitelist">
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

			<DetailRow label="Реестр РКН">
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
				<DetailRow label="Заблокированный домен">
					<span class={valueClass}>{result.domain}</span>
				</DetailRow>
			{/if}

			{#if result.blockedSubnets.length > 0 && !result.asnInfo}
				<DetailRow label="Заблокированные подсети">
					<div>
						{#each result.blockedSubnets as network}
							<p class={valueClass}>{network}</p>
						{/each}
					</div>
				</DetailRow>
			{/if}

			{#if result.asnInfo && blockedPrefixes.length > 0}
				<DetailRow label="Заблокированные подсети ASN">
					<ResultStringList items={blockedPrefixes} alert />
				</DetailRow>
			{/if}
		</div>
	</div>

	{#if result.id}
		<Feedback id={result.id} {theme} />
	{/if}
</div>
