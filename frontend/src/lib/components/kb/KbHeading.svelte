<script lang="ts">
import { onMount, type Snippet } from "svelte";
import { getKbContext } from "$lib/context/kb.svelte";

let {
	children,
	id,
	title,
}: {
	children?: Snippet;
	id: string;
	title?: string;
} = $props();

const kb = getKbContext();

onMount(() => {
	if (kb) {
		kb.registerHeading({ id, title: title || id });
	}
});
</script>

<h2
	{id}
	class="relative mt-12 mb-6 text-3xl font-bold tracking-tight text-neutral-100 group"
>
	<a href={`#${id}`} class="no-underline">
		<span
			class="absolute -left-8 top-0 opacity-0 transition-opacity group-hover:opacity-50"
			aria-hidden="true"
		>
			#
		</span>
		{#if children}
			{@render children()}
		{:else}
			{title}
		{/if}
	</a>
</h2>
