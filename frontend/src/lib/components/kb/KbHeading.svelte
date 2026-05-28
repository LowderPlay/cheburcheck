<script lang="ts">
import type { Snippet } from "svelte";

let { children, id }: { children: Snippet; id: string } = $props();
</script>

<h2
	{id}
	class="mt-10 mb-4 text-[32px] leading-[1.2] font-bold tracking-[0.1em] text-neutral-100 uppercase"
>
	<a href={`#${id}`} class="no-underline hover:underline">
		{@render children()}
	</a>
</h2>
