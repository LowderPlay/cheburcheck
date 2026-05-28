<script lang="ts">
import type { Snippet } from "svelte";

let {
	children,
	label,
	href,
}: { children: Snippet; label: string; href?: string } = $props();
</script>

<div
	class="flex flex-col gap-1 border-b border-neutral-800 py-3 last:border-b-0 sm:flex-row sm:items-center sm:justify-between"
>
	{#if href}
		<a
			{href}
			class="text-xs tracking-wider text-neutral-500 uppercase underline decoration-neutral-500"
		>
			{label}
		</a>
	{:else}
		<span class="text-xs tracking-wider text-neutral-500 uppercase">
			{label}
		</span>
	{/if}
	<div class="flex justify-end">
		{@render children()}
	</div>
</div>
