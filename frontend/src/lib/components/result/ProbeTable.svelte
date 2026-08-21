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
	TriangleAlert,
} from "@lucide/svelte";
import {
	type DnsObservation,
	displayProbeVerdicts,
	type ProbeResult,
	type ProbeStatus,
} from "$lib/api/probe";

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

function dnsObservationStatus(
	observation: DnsObservation,
): "ok" | "spoofed" | "error" {
	if (observation.outcome.type === "Error") return "error";
	return observation.suspected_spoofing ? "spoofed" : "ok";
}

const dnsProtocolLabels: Record<DnsObservation["protocol"], string> = {
	Udp: "UDP",
	Tcp: "TCP",
	Doh: "DoH",
	Dot: "DoT",
};

const dnsProtocols = ["Udp", "Tcp", "Doh", "Dot"] as const;

function groupDnsObservations(observations: DnsObservation[]) {
	const providers = new Map<
		string,
		Partial<Record<DnsObservation["protocol"], DnsObservation>>
	>();
	for (const observation of observations) {
		const protocols = providers.get(observation.provider) ?? {};
		protocols[observation.protocol] = observation;
		providers.set(observation.provider, protocols);
	}
	return [...providers].map(([provider, protocols]) => ({
		provider,
		protocols,
	}));
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
	dns_spoofing: {
		icon: CircleX,
		text: "Подмена DNS",
		class: "text-red-500",
		bg: "bg-red-500/10",
		border: "border-red-500/20",
	},
	whitelist: {
		icon: ShieldCheck,
		text: "Исключение для CDN",
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
				Сканеров онлайн: {status.online_probes}
			</div>
			<div class="flex items-center gap-1">
				Получено ответов: {probes.length} / {status.online_probes}
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
						{@const verdicts = displayProbeVerdicts(probe, isStaticBlocked)}
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
								<div class="flex flex-wrap gap-1.5">
									{#each verdicts as verdict}
										{@const style = verdictStyles[verdict]}
										<a
											href={verdict === "whitelist" ? "/kb/whitelist" : "/kb/probing"}
											onclick={(event) => event.stopPropagation()}
											class={`inline-flex items-center gap-1.5 px-2 py-1 rounded border ${style.bg} ${style.border} ${style.class} text-xs font-bold transition-opacity hover:opacity-80`}
										>
											<style.icon size={14} />
											{style.text}
										</a>
									{/each}
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
									{#if probe.verdicts.includes("tspu_block") && probe.dpi_hop !== null}
										<div
											class="mb-3 flex items-center gap-2 rounded-md border border-red-500/50 bg-red-500/15 px-3 py-2 text-red-200"
										>
											<TriangleAlert size={18} class="shrink-0 text-red-400" />
											<span class="font-bold">
												Блокировка ТСПУ обнаружена после
												{probe.dpi_hop}
												прыжка
											</span>
										</div>
									{:else if probe.target_hop !== null}
										<div class="text-md text-neutral-200 mb-3">
											Блокировка IP на ТСПУ <b>не обнаружена</b>
										</div>
									{/if}
									{#if probe.dns}
										<div class="mb-4">
											<div
												class="border-t border-neutral-800 py-2 flex items-center justify-between gap-3"
											>
												<h4
													class="text-xs font-bold uppercase tracking-wide text-neutral-300"
												>
													DNS-проверка
												</h4>
												<span
													class={`text-xs font-semibold ${probe.dns.spoofing_detected ? 'text-red-400' : probe.dns.suspicious_provider_count > 0 ? 'text-amber-400' : 'text-green-400'}`}
												>
													{probe.dns.spoofing_detected
												? `Возможна подмена (${probe.dns.suspicious_provider_count}/${probe.dns.verdict_threshold})`
												: probe.dns.suspicious_provider_count > 0
													? `Недостаточно подтверждений (${probe.dns.suspicious_provider_count}/${probe.dns.verdict_threshold})`
													: "Подмена не выявлена"}
												</span>
											</div>
											<div
												class="overflow-x-auto rounded border border-neutral-700/50"
											>
												<table class="w-full text-left text-xs">
													<thead class="bg-neutral-800/60 text-neutral-400">
														<tr>
															<th class="px-3 py-2 font-semibold">Провайдер</th>
															{#each dnsProtocols as protocol}
																<th class="px-3 py-2 font-semibold">
																	{dnsProtocolLabels[protocol]}
																</th>
															{/each}
														</tr>
													</thead>
													<tbody>
														{#each groupDnsObservations(probe.dns.observations) as provider}
															<tr class="border-t border-neutral-800/80">
																<td
																	class="px-3 py-2 font-semibold capitalize text-neutral-200"
																>
																	{provider.provider}
																</td>
																{#each dnsProtocols as protocol}
																	{@const observation = provider.protocols[protocol]}
																	<td class="px-3 py-2">
																		{#if observation}
																			{@const dnsStatus = dnsObservationStatus(observation)}
																			<div
																				class={dnsStatus === "spoofed" ? "text-red-400" : dnsStatus === "error" ? "text-amber-400" : "text-green-400"}
																			>
																				<div class="font-semibold">
																					{dnsStatus === "spoofed" ? "Подозрительно" : dnsStatus === "error" ? "Ошибка" : "Норма"}
																				</div>
																				<div
																					class="mt-0.5 whitespace-nowrap font-mono text-[10px] text-neutral-500"
																				>
																					{observation.metadata.response_codes.join(", ") || "—"}
																					·
																					{observation.metadata.ipv4_count}/{observation.metadata.ipv6_count}
																				</div>
																			</div>
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
											<p class="mt-2 text-[11px] text-neutral-500">
												Сравниваются код ответа и количество уникальных
												IPv4/IPv6. Выполнено по
												{probe.dns.samples_per_protocol || 1}
												запроса на протокол. Вердикт требует подтверждения от
												{probe.dns.verdict_threshold || 2}
												DNS-провайдеров.
											</p>
										</div>
									{/if}
									{#if probe.host_results?.length}
										<div
											class="border-t border-neutral-800 py-2 flex items-center justify-between gap-3"
										>
											<h4
												class="text-xs font-bold uppercase tracking-wide text-neutral-300"
											>
												CDN-проверка
											</h4>
										</div>
										<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
											{#each probe.host_results as host}
												<div
													class="flex items-center justify-between p-2 rounded bg-neutral-800/30 border border-neutral-700/30"
												>
													<div class="flex flex-col">
														<span class="text-xs font-bold text-neutral-400">
															Сервер {host.host_id}
															({host.host === "Blacklist" ? "в заблокированных" : "в доступных"}
															диапазонах)
														</span>
														<span class="text-xs text-neutral-200">
															{#if host.probe_evidence.type === 'Good'}
																Успешно
															{:else if host.probe_evidence.type === 'ClientHello'}
																Блокировка после ClientHello
															{:else if host.probe_evidence.type === 'DataTimeout'}
																Таймаут получения данных, получено
																{host.probe_evidence.bytes}
																байт
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
									{/if}
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
