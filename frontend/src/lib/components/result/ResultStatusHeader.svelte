<script lang="ts">
import {
	CircleQuestionMark,
	ShieldAlert,
	ShieldCheck,
	ShieldX,
} from "@lucide/svelte";
import type { DisplayProbeVerdict } from "$lib/api/probe";

type ResultVerdict = DisplayProbeVerdict | "blocked";

let {
	verdict,
}: {
	verdict: ResultVerdict;
} = $props();

const labels: Record<ResultVerdict, { title: string; subtitle: string }> = {
	blocked: {
		title: "Заблокирован",
		subtitle: "Ресурс был найден в списках блокировок",
	},
	tspu_block: {
		title: "Блокировка IP",
		subtitle: "Сканеры обнаружили блокировку TCP на уровне ТСПУ",
	},
	sni_block: {
		title: "Блокировка SNI",
		subtitle: "Сканеры обнаружили блокировку по имени домена в SNI",
	},
	dns_spoofing: {
		title: "Блокировка DNS",
		subtitle: "Сканеры обнаружили подмену ответов DNS для данного домена",
	},
	whitelist: {
		title: "Исключение из CDN",
		subtitle:
			"Домен снимает ограничение 16-20 КБ при подключении к заблокированным CDN",
	},
	cdn_block: {
		title: "Блокировка CDN",
		subtitle: "Сканеры обнаружили блокировку CDN (16-20 КБ)",
	},
	ok: {
		title: "Доступен",
		subtitle: "Ограничений не обнаружено",
	},
	uncertain: {
		title: "Неясно",
		subtitle: "Сканеры не смогли определить доступность ресурса",
	},
};
const title = $derived(labels[verdict].title);
const subtitle = $derived(labels[verdict].subtitle);
const accentClass = $derived(
	verdict === "whitelist"
		? "text-[#f0b100]"
		: verdict === "ok"
			? "text-green-500"
			: verdict === "uncertain"
				? "text-neutral-400"
				: "text-red-500",
);
const StatusIcon = $derived(
	verdict === "whitelist"
		? ShieldAlert
		: verdict === "ok"
			? ShieldCheck
			: verdict === "uncertain"
				? CircleQuestionMark
				: ShieldX,
);
</script>

<div class="flex items-center gap-4">
	<div
		class={`flex items-center rounded-lg justify-center border border-current bg-white/5 p-3 ${accentClass}`}
	>
		<StatusIcon size={32} aria-hidden="true" />
	</div>
	<div>
		<h2 class={`text-2xl leading-tight font-bold uppercase ${accentClass}`}>
			{title}
		</h2>
		<p class="text-sm opacity-80">{subtitle}</p>
	</div>
</div>
