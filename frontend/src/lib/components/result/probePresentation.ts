import {
	CircleCheck,
	CircleQuestionMark,
	CircleX,
	ShieldCheck,
} from "@lucide/svelte";
import {
	type DisplayProbeVerdict,
	displayProbeVerdicts,
	type ProbeResult,
} from "$lib/api/probe";

export const verdicts = {
	tspu_block: {
		label: "ТСПУ блок",
		color: "text-red-400",
		bar: "bg-red-500",
		icon: CircleX,
	},
	sni_block: {
		label: "SNI блок",
		color: "text-red-400",
		bar: "bg-red-500",
		icon: CircleX,
	},
	dns_spoofing: {
		label: "Подмена DNS",
		color: "text-red-400",
		bar: "bg-red-500",
		icon: CircleX,
	},
	whitelist: {
		label: "Исключение для CDN",
		color: "text-amber-400",
		bar: "bg-amber-500",
		icon: ShieldCheck,
	},
	cdn_block: {
		label: "CDN блок (16–20)",
		color: "text-red-400",
		bar: "bg-red-500",
		icon: CircleX,
	},
	ok: {
		label: "Доступен",
		color: "text-green-400",
		bar: "bg-green-500",
		icon: CircleCheck,
	},
	uncertain: {
		label: "Неясно",
		color: "text-neutral-400",
		bar: "bg-neutral-500",
		icon: CircleQuestionMark,
	},
} as const;
export const verdictOrder: DisplayProbeVerdict[] = [
	"tspu_block",
	"sni_block",
	"dns_spoofing",
	"whitelist",
	"cdn_block",
	"ok",
	"uncertain",
];

export const regionName = (probe: ProbeResult) =>
	probe.region?.trim() || "Регион не указан";
export const scannerName = (probe: ProbeResult) =>
	[probe.provider?.trim(), probe.asn?.trim()].filter(Boolean).join(" · ") ||
	`Сканер ${probe.probe_id}`;

export const voteCount = (
	members: ProbeResult[],
	verdict: DisplayProbeVerdict,
	isStaticCdn: boolean,
) =>
	members.filter((probe) =>
		displayProbeVerdicts(probe, isStaticCdn).includes(verdict),
	).length;
