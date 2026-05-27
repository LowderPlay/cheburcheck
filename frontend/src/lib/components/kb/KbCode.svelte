<script lang="ts">
import type { Snippet } from "svelte";

let { children }: { children: Snippet } = $props();
</script>

<pre
	class="my-4 overflow-x-scroll border-l-2 border-neutral-800 pl-4 font-[inherit] text-neutral-500"
>{@render children()}</pre>
