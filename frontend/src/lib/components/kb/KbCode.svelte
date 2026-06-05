<script lang="ts">
import type { Snippet } from "svelte";

let { children }: { children: Snippet } = $props();
</script>

<pre
	class="my-8 overflow-x-auto rounded-lg border border-neutral-800 bg-neutral-900/50 p-6 font-mono text-sm leading-relaxed text-neutral-300 shadow-inner"
>{@render children()}</pre>
