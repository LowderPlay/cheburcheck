<script lang="ts">
import { ShieldAlert, ShieldCheck, ShieldX } from "@lucide/svelte";

type ResultTheme = "blocked" | "clean" | "whitelist";

let {
	theme,
	blocked,
}: {
	theme: ResultTheme;
	blocked: boolean;
} = $props();

const title = $derived(
	theme === "whitelist"
		? "Белый список"
		: theme === "blocked"
			? "Заблокирован"
			: "Доступен",
);
const subtitle = $derived(
	theme === "whitelist"
		? "Ресурс находится в белом списке"
		: theme === "blocked"
			? "Ресурс был найден в списках блокировок"
			: "Ограничений не обнаружено",
);
const accentClass = $derived(
	theme === "whitelist"
		? "text-[#f0b100]"
		: theme === "blocked"
			? "text-red-500"
			: "text-green-500",
);
const StatusIcon = $derived(
	theme === "whitelist" ? ShieldAlert : blocked ? ShieldX : ShieldCheck,
);
</script>

<div class="mb-8 flex gap-4">
	<div
		class={`flex items-center justify-center border border-current bg-white/5 p-3 ${accentClass}`}
	>
		<StatusIcon size={32} aria-hidden="true" />
	</div>
	<div>
		<h2
			class={`mb-1 text-3xl leading-[1.2] font-bold tracking-widest uppercase ${accentClass}`}
		>
			{title}
		</h2>
		<p class={`text-sm ${accentClass}`}>{subtitle}</p>
	</div>
</div>
