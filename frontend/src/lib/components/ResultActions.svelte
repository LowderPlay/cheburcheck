<script lang="ts">
import { Check, Clipboard, Download, RefreshCw } from "@lucide/svelte";
import { page } from "$app/state";
import type { CheckResult } from "$lib/api/check";
import type {
	ProbeResult,
	ProbeStatus,
	ResolvedProbeVerdict,
} from "$lib/api/probe";
import { createSharedResult } from "$lib/api/share";

let {
	result,
	probes,
	probeStatus,
	probeVerdict = null,
	onRetryProbe,
}: {
	result: CheckResult;
	probes: ProbeResult[];
	probeStatus: ProbeStatus | null;
	probeVerdict?: ResolvedProbeVerdict | null;
	onRetryProbe: () => void;
} = $props();

let shareCopied = $state(false);
let reportCopied = $state(false);
let copyError = $state(false);
let shareError = $state("");
let sharePending = $state(false);
let shareUrl = $state("");
let resetCopyTimer: ReturnType<typeof setTimeout> | undefined;

const verdictNames: Record<string, string> = {
	cdn_block: "Блокировка CDN",
	uncertain: "Неопределённый результат",
	dns_spoofing: "Подмена DNS",
	sni_block: "Блокировка SNI",
	tspu_block: "Блокировка ТСПУ",
	whitelist: "Белый список",
	ok: "Ограничений не обнаружено",
};

const staticWhitelist = $derived(!!result.whitelist && !result.domain);
const verdict = $derived(
	probeVerdict
		? verdictNames[probeVerdict]
		: staticWhitelist
			? "Исключение для CDN"
			: result.found
				? "Найден в списках"
				: "Не найден в списках",
);
const blocked = $derived(
	probeVerdict
		? ["cdn_block", "dns_spoofing", "sni_block", "tspu_block"].includes(
				probeVerdict,
			)
		: result.found && !staticWhitelist,
);

const reportData = $derived.by(() => ({
	target: { value: result.target, type: result.targetType },
	generated_at: new Date().toISOString(),
	verdict: {
		blocked,
		label: verdict,
	},
	registry: {
		domain: result.domain,
		ips: result.ips,
		blocked_subnets: result.blockedSubnets,
		asn_blocked_prefixes: result.asnInfo?.blockedPrefixes ?? [],
		whitelist: result.whitelist
			? { found: true, last_ok: result.whitelist.lastOk ?? null }
			: { found: false, last_ok: null },
		geo: result.geo,
		cdn_providers: result.providers.map((provider) => ({
			name: provider.name,
			networks: provider.networks.map((network) => network.cidr),
		})),
	},
	dynamic_check: {
		status: probeStatus?.status ?? "unavailable",
		responses_received: probes.length,
		responses_expected: probeStatus?.online_probes ?? 0,
		probes: probes.map((probe) => ({
			probe_id: probe.probe_id,
			region: probe.region ?? null,
			provider: probe.provider ?? null,
			asn: probe.asn ?? null,
			verdicts: probe.verdicts,
			host_results: probe.host_results ?? [],
			dns: probe.dns,
		})),
	},
}));

const report = $derived.by(() => {
	const { target, verdict: reportVerdict, registry, dynamic_check } = reportData;
	const lines = [
		`Проверка: ${target.value}`,
		`Дата отчёта: ${new Intl.DateTimeFormat("ru-RU", { dateStyle: "medium", timeStyle: "short" }).format(new Date(reportData.generated_at))}`,
		`Статус по спискам: ${reportVerdict.label}`,
		`Тип запроса: ${target.type}`,
		"",
		"Данные реестра и сетей:",
		`Домен в реестре: ${registry.domain ?? "не найден"}`,
		`IP-адреса: ${registry.ips.length ? registry.ips.join(", ") : "нет данных"}`,
		`Заблокированные подсети: ${registry.blocked_subnets.length ? registry.blocked_subnets.join(", ") : "не найдены"}`,
		`Подсети ASN в списках: ${registry.asn_blocked_prefixes.length ? registry.asn_blocked_prefixes.join(", ") : "не найдены"}`,
		`Белый список: ${registry.whitelist.found ? `найден${registry.whitelist.last_ok ? `, последнее подтверждение ${new Date(registry.whitelist.last_ok).toLocaleDateString("ru-RU")}` : ""}` : "не найден"}`,
		`ASN: ${registry.geo.asn ?? "нет данных"}`,
		`Хостинг / ISP: ${registry.geo.organisation || "нет данных"}`,
		`Локация: ${registry.geo.location || "нет данных"}`,
		`CDN-провайдеры: ${registry.cdn_providers.length ? registry.cdn_providers.map((provider) => `${provider.name} (${provider.networks.join(", ")})`).join("; ") : "не найдены"}`,
		"",
		"Динамическая проверка:",
		`Статус: ${dynamic_check.status === "done" ? "завершена" : dynamic_check.status === "error" ? "соединение прервано или произошла ошибка" : dynamic_check.status === "unavailable" ? "недоступна" : "выполняется"}`,
		`Ответили сканеры: ${dynamic_check.responses_received}${probeStatus ? ` из ${dynamic_check.responses_expected}` : ""}`,
	];

	if (dynamic_check.probes.length) {
		lines.push("Ответы сканеров:");
		for (const probe of dynamic_check.probes) {
			const details = [probe.region, probe.provider, probe.asn].filter(Boolean);
			const verdicts = probe.verdicts.map((item) => verdictNames[item] ?? item);
			lines.push(
				`- ${details.length ? details.join(" · ") : probe.probe_id}: ${verdicts.length ? verdicts.join(", ") : "нет вердикта"}`,
			);
			if (probe.host_results.length) {
				lines.push(
					`  Хосты: ${probe.host_results.map((host) => `${host.host_id} — ${host.probe_evidence.type}${host.probe_evidence.type === "DataTimeout" ? ` (${host.probe_evidence.bytes} байт)` : ""}`).join("; ")}`,
				);
			}
			if (probe.dns) {
				lines.push(
					`  DNS: ${probe.dns.spoofing_detected ? "обнаружены признаки подмены" : "подмена не обнаружена"}; проверено провайдеров: ${probe.dns.observations.length}`,
				);
			}
		}
	}

	return lines.join("\n");
});

async function copyText(value: string) {
	if (navigator.clipboard?.writeText) {
		await navigator.clipboard.writeText(value);
		return;
	}

	const textarea = document.createElement("textarea");
	textarea.value = value;
	textarea.style.position = "fixed";
	textarea.style.opacity = "0";
	document.body.append(textarea);
	textarea.select();
	const copied = document.execCommand("copy");
	textarea.remove();
	if (!copied) throw new Error("Clipboard unavailable");
}

async function shareResult() {
	copyError = false;
	shareError = "";
	if (sharePending) return;

	try {
		if (!result.id) throw new Error("Для этого результата нельзя создать ссылку");
		shareUrl = "";
		shareCopied = false;
		sharePending = true;
		const shared = await createSharedResult(result.id);
		shareUrl = new URL(`/share/${shared.id}`, page.url.origin).href;
		await copyText(shareUrl);
		shareCopied = true;
		if (resetCopyTimer) clearTimeout(resetCopyTimer);
		resetCopyTimer = setTimeout(() => {
			shareCopied = false;
			reportCopied = false;
		}, 2000);
	} catch (cause) {
		if (shareUrl) copyError = true;
		else {
			shareError =
				cause instanceof Error
					? cause.message
					: "Не удалось создать ссылку. Попробуйте ещё раз.";
		}
	} finally {
		sharePending = false;
	}
}

async function copyReport() {
	copyError = false;
	try {
		await copyText(report);
		reportCopied = true;
		if (resetCopyTimer) clearTimeout(resetCopyTimer);
		resetCopyTimer = setTimeout(() => {
			shareCopied = false;
			reportCopied = false;
		}, 2000);
	} catch {
		copyError = true;
	}
}

function escapeXml(value: string) {
	return value
		.replace(/&/g, "&amp;")
		.replace(/</g, "&lt;")
		.replace(/>/g, "&gt;")
		.replace(/\"/g, "&quot;")
		.replace(/'/g, "&apos;");
}

function toXml(value: unknown, name: string, depth = 1): string {
	const indent = "\t".repeat(depth);
	if (Array.isArray(value)) {
		if (!value.length) return `${indent}<${name}/>\n`;
		return `${indent}<${name}>\n${value.map((item) => toXml(item, "item", depth + 1)).join("")}${indent}</${name}>\n`;
	}
	if (value && typeof value === "object") {
		const entries = Object.entries(value);
		if (!entries.length) return `${indent}<${name}/>\n`;
		return `${indent}<${name}>\n${entries.map(([key, item]) => toXml(item, key, depth + 1)).join("")}${indent}</${name}>\n`;
	}
	if (value === null || value === undefined) return `${indent}<${name}/>\n`;
	return `${indent}<${name}>${escapeXml(String(value))}</${name}>\n`;
}

function downloadReport(format: "txt" | "json" | "xml") {
	const contents = {
		txt: report,
		json: `${JSON.stringify(reportData, null, 2)}\n`,
		xml: `<?xml version="1.0" encoding="UTF-8"?>\n${toXml(reportData, "check-report", 0)}`,
	};
	const mimeTypes = {
		txt: "text/plain;charset=utf-8",
		json: "application/json;charset=utf-8",
		xml: "application/xml;charset=utf-8",
	};
	const blob = new Blob([contents[format]], { type: mimeTypes[format] });
	const url = URL.createObjectURL(blob);
	const link = document.createElement("a");
	link.href = url;
	link.download = `cheburcheck-${result.target.replace(/[^a-z0-9.-]+/gi, "_")}.${format}`;
	link.click();
	URL.revokeObjectURL(url);
}
</script>

<div class="mb-4 flex flex-wrap items-center gap-2">
	<button
		type="button"
		class="flex cursor-pointer items-center gap-2 border border-neutral-700 bg-neutral-900 px-3 py-2 text-xs font-bold text-neutral-200 transition-colors hover:bg-neutral-800"
		disabled={sharePending}
		onclick={shareResult}
	>
		{#if shareCopied}<Check size={14} aria-hidden="true" />{:else}<Clipboard size={14} aria-hidden="true" />{/if}
		{sharePending ? "Создаём ссылку…" : shareCopied ? "Ссылка скопирована" : "Поделиться результатом"}
	</button>
	<button
		type="button"
		class="flex cursor-pointer items-center gap-2 border border-neutral-700 bg-neutral-900 px-3 py-2 text-xs font-bold text-neutral-200 transition-colors hover:bg-neutral-800"
		onclick={copyReport}
	>
		{#if reportCopied}<Check size={14} aria-hidden="true" />{:else}<Clipboard size={14} aria-hidden="true" />{/if}
		{reportCopied ? "Сводка скопирована" : "Скопировать сводку"}
	</button>
	{#each ["txt", "json", "xml"] as format}
		<button
			type="button"
			class="flex cursor-pointer items-center gap-2 border border-neutral-700 bg-neutral-900 px-3 py-2 text-xs font-bold text-neutral-200 transition-colors hover:bg-neutral-800"
			onclick={() => downloadReport(format as "txt" | "json" | "xml")}
		>
			<Download size={14} aria-hidden="true" />
			Скачать {format.toUpperCase()}
		</button>
	{/each}
	{#if probeStatus}
		<button
			type="button"
			class="flex cursor-pointer items-center gap-2 border border-neutral-700 bg-neutral-900 px-3 py-2 text-xs font-bold text-neutral-200 transition-colors hover:bg-neutral-800 disabled:cursor-not-allowed disabled:opacity-50"
			disabled={probeStatus.status === "started" || probeStatus.status === "progress"}
			onclick={onRetryProbe}
		>
			<RefreshCw size={14} aria-hidden="true" />
			Повторить динамическую проверку
		</button>
	{/if}
</div>
{#if shareError}
	<p class="mb-3 text-xs text-red-400" role="status">{shareError}</p>
{/if}
{#if shareUrl && copyError}
	<p class="mb-3 break-all text-xs text-neutral-400" role="status">
		Ссылка создана, но не скопирована:
		<a class="underline" href={shareUrl}>{shareUrl}</a>
	</p>
{/if}
{#if copyError}
	<p class="mb-3 text-xs text-red-400" role="status">Не удалось скопировать в буфер обмена.</p>
{/if}
