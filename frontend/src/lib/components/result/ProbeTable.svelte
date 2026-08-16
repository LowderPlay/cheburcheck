<script lang="ts">
import {
	Activity,
	ChevronDown,
	ChevronUp,
	CircleCheck,
	CircleQuestionMark,
	CircleX,
	LoaderCircle,
	ShieldCheck,
} from "@lucide/svelte";
import type { ProbeResult, ProbeStatus } from "$lib/api/probe";

let {
	probes,
	status,
	isStaticBlocked,
}: {
	probes: ProbeResult[];
	status: ProbeStatus;
	isStaticBlocked: boolean;
} = $props();

let expandedRows = $state<Record<string, boolean>>({});

function toggleRow(id: string) {
	expandedRows[id] = !expandedRows[id];
}

const verdictStyles = {
	ok: {
		icon: CircleCheck,
		text: "Доступен",
		class: "text-green-500",
		bg: "bg-green-500/10",
		border: "border-green-500/20",
	},
	cdn_block: {
		icon: CircleX,
		text: "CDN Блок (16-20)",
		class: "text-red-500",
		bg: "bg-red-500/10",
		border: "border-red-500/20",
	},
	sni_block: {
		icon: CircleX,
		text: "SNI Блок",
		class: "text-red-500",
		bg: "bg-red-500/10",
		border: "border-red-500/20",
	},
	tspu_block: {
		icon: CircleX,
		text: "ТСПУ Блок",
		class: "text-red-500",
		bg: "bg-red-500/10",
		border: "border-red-500/20",
	},
	whitelist: {
		icon: ShieldCheck,
		text: "Белый список",
		class: "text-amber-500",
		bg: "bg-amber-500/10",
		border: "border-amber-500/20",
	},
	uncertain: {
		icon: CircleQuestionMark,
		text: "Неясно",
		class: "text-neutral-400",
		bg: "bg-neutral-400/10",
		border: "border-neutral-400/20",
	},
};
</script>

<div class="mt-8 space-y-4">
	<div
		class="flex items-center justify-between border-b border-neutral-800 pb-2"
	>
		<h3 class="text-sm font-bold text-white uppercase flex items-center gap-2">
			<Activity size={16} class="text-primary" />
			<a
				class="underline decoration-dotted underline-offset-2"
				href="/kb/probing"
			>
				Результаты динамической проверки
			</a>
		</h3>
		<div class="text-xs text-neutral-400 flex items-center gap-3">
			<div class="flex items-center gap-1">
				<span
					class={`w-2 h-2 rounded-full ${status.online_probes > 0 ? 'bg-green-500 animate-pulse' : 'bg-neutral-600'}`}
				></span>
				Сканеров онлайн:{status.online_probes}
			</div>
			<div class="flex items-center gap-1">
				Получено ответов:{probes.length} /{status.online_probes}
			</div>
		</div>
	</div>

	{#if status.online_probes > 0 && probes.length < status.online_probes && status.status !== 'done'}
		<div class="h-1 w-full bg-neutral-800 rounded-full overflow-hidden">
			<div
				class="h-full bg-primary transition-all duration-500 ease-out"
				style:width={`${(probes.length / status.online_probes) * 100}%`}
			></div>
		</div>
	{/if}

	{#if probes.length === 0 && status.status !== 'done'}
		<div
			class="flex flex-col items-center justify-center py-12 border border-neutral-800 bg-neutral-900/20 rounded-lg"
		>
			<LoaderCircle class="animate-spin text-primary mb-4" size={32} />
			<p class="text-neutral-400 text-sm">Ожидание ответов от сканеров...</p>
		</div>
	{:else}
		<div
			class="overflow-x-auto border border-neutral-800 rounded-lg bg-neutral-900/10"
		>
			<table class="w-full text-left text-sm border-collapse">
				<thead>
					<tr class="border-b border-neutral-800 bg-neutral-900/40">
						<th class="p-3 font-semibold text-neutral-300">Регион</th>
						<th class="p-3 font-semibold text-neutral-300">Провайдер / AS</th>
						<th class="p-3 font-semibold text-neutral-300">Вердикт</th>
						<th class="p-3 w-10"></th>
					</tr>
				</thead>
				<tbody>
					{#each probes as probe (probe.probe_id)}
						{@const style = verdictStyles[(isStaticBlocked && probe.host_results?.length !== 0 && probe.verdict === "ok") ? "cdn_block" : probe.verdict]}
						{@const isExpanded = !!expandedRows[probe.probe_id]}
						<tr
							class="border-b border-neutral-800/50 hover:bg-neutral-800/20 transition-colors cursor-pointer select-none"
							onclick={() => toggleRow(probe.probe_id)}
							onkeydown={(e) => e.key === 'Enter' && toggleRow(probe.probe_id)}
							tabindex="0"
						>
							<td class="p-3">
								<div class="font-medium text-neutral-200">
									{probe.region || "Неизвестно"}
								</div>
							</td>
							<td class="p-3">
								<div class="text-neutral-200">{probe.provider || "-"}</div>
								<div class="text-xs text-neutral-500">{probe.asn || ""}</div>
							</td>
							<td class="p-3">
								<div
									class={`inline-flex items-center gap-1.5 px-2 py-1 rounded border ${style.bg} ${style.border} ${style.class} text-xs font-bold`}
								>
									<style.icon size={14} />
									{style.text}
								</div>
							</td>
							<td class="p-3 text-right">
								{#if isExpanded}
									<ChevronUp size={16} class="text-neutral-500" />
								{:else}
									<ChevronDown size={16} class="text-neutral-500" />
								{/if}
							</td>
						</tr>
						{#if isExpanded}
							<tr class="bg-neutral-900/30">
								<td colspan="4" class="p-4 border-b border-neutral-800/50">
									<div class="text-md text-neutral-200 mb-2">
										Блокировка на ТСПУ<b>
											{probe.verdict === "tspu_block" ? ` обнаружена после ${probe.target_hop} прыжка` : " не обнаружена"}
										</b>
									</div>
									<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
										{#each probe.host_results as host}
											<div
												class="flex items-center justify-between p-2 rounded bg-neutral-800/30 border border-neutral-700/30"
											>
												<div class="flex flex-col">
													<span class="text-xs font-bold text-neutral-400">
														Сервер{host.host_id}
														({host.host === "Blacklist" ? "в заблокированных" : "в доступных"} диапазонах)
													</span>
													<span class="text-xs text-neutral-200">
														{#if host.probe_evidence.type === 'Good'}
															Успешно
														{:else if host.probe_evidence.type === 'ClientHello'}
															Блокировка после ClientHello
														{:else if host.probe_evidence.type === 'DataTimeout'}
															Таймаут получения данных, получено{host.probe_evidence.bytes} байт
														{:else if host.probe_evidence.type === 'ConnectionError'}
															Ошибка подключения
														{/if}
													</span>
												</div>
												{#if host.probe_evidence.type === 'Good'}
													<CircleCheck size={14} class="text-green-500" />
												{:else if host.probe_evidence.type === 'ClientHello'}
													<CircleX size={14} class="text-red-500" />
												{:else if host.probe_evidence.type === 'DataTimeout'}
													<CircleX size={14} class="text-orange-500" />
												{:else}
													<CircleQuestionMark
														size={14}
														class="text-neutral-500"
													/>
												{/if}
											</div>
										{/each}
									</div>
								</td>
							</tr>
						{/if}
					{/each}
				</tbody>
			</table>
		</div>
	{/if}

	{#if status.status === 'done' && probes.length === 0}
		<div
			class="p-6 border border-neutral-800 bg-neutral-900/20 text-center rounded-lg"
		>
			<p class="text-neutral-400 text-sm">
				Сканеры не ответили на запрос или недоступны.
			</p>
		</div>
	{/if}
</div>
