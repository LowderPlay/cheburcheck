<script lang="ts">
import { Activity, Globe, Server } from "@lucide/svelte";

let {
	icon,
	label,
	value,
}: {
	icon: "activity" | "globe" | "server";
	label: string;
	value: string;
} = $props();

const icons = {
	activity: Activity,
	globe: Globe,
	server: Server,
} as const;

const StatIcon = $derived(icons[icon]);
</script>

<div class="flex flex-col gap-2 border border-neutral-800 p-3">
	<div class="flex gap-2">
		<StatIcon size={16} aria-hidden="true" />
		<span>{label}</span>
	</div>
	<span>{value}</span>
</div>
