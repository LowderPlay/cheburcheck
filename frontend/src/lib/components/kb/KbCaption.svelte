<script lang="ts">
import type { Snippet } from "svelte";

let { children }: { children: Snippet } = $props();
</script>

<p
	class="mt-6 mb-4 text-center text-sm font-medium tracking-wide text-neutral-500 uppercase"
>
	{@render children()}
</p>
