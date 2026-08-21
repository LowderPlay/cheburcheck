<script lang="ts">
import { ShieldAlert, ShieldCheck, ShieldX } from "@lucide/svelte";
import type { ResolvedProbeVerdict } from "$lib/api/probe";

type ResultVerdict = ResolvedProbeVerdict | "blocked";

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
		title: "Заблокирован",
		subtitle: "Сканеры обнаружили блокировку TCP на уровне ТСПУ",
	},
	sni_block: {
		title: "Заблокирован",
		subtitle: "Сканеры обнаружили блокировку по имени домена в SNI",
	},
	dns_spoofing: {
		title: "Заблокирован",
		subtitle: "Сканеры обнаружили подмену ответов DNS для данного домена",
	},
	whitelist: {
		title: "Исключение для CDN",
		subtitle:
			"Домен снимает ограничение 16-20 КБ при подключении к заблокированным CDN",
	},
	cdn_block: {
		title: "Заблокирован",
		subtitle: "Сканеры обнаружили блокировку CDN (16-20 КБ)",
	},
	ok: {
		title: "Не ограничен",
		subtitle: "Ограничений не обнаружено",
	},
};
const title = $derived(labels[verdict].title);
const subtitle = $derived(labels[verdict].subtitle);
const accentClass = $derived(
	verdict === "whitelist"
		? "text-[#f0b100]"
		: verdict === "ok"
			? "text-green-500"
			: "text-red-500",
);
const StatusIcon = $derived(
	verdict === "whitelist"
		? ShieldAlert
		: verdict === "ok"
			? ShieldCheck
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
