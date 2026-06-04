<script lang="ts">
import type { Component, Snippet } from "svelte";

let {
	children,
	label,
	href,
	icon: Icon,
}: {
	children: Snippet;
	label: string;
	href?: string;
	icon?: Component;
} = $props();
</script>

<div
	class="flex flex-col gap-1 border-b border-neutral-800 py-3 last:border-b-0 sm:flex-row sm:items-center sm:justify-between"
>
	<div class="flex items-center gap-2">
		{#if Icon}
			<Icon size={14} class="text-neutral-500" aria-hidden="true" />
		{/if}
		{#if href}
			<a
				{href}
				class="text-xs font-medium text-neutral-400 uppercase underline decoration-neutral-400/50 hover:text-neutral-300"
			>
				{label}
			</a>
		{:else}
			<span class="text-xs font-medium text-neutral-400 uppercase">
				{label}
			</span>
		{/if}
	</div>
	<div class="flex justify-end">
		{@render children()}
	</div>
</div>
