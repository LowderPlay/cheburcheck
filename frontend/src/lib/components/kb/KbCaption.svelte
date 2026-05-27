<script lang="ts">
import type { Snippet } from "svelte";

let { children }: { children: Snippet } = $props();
</script>

<i class="mt-6 mb-3 block text-center text-neutral-400">
	{@render children()}
</i>
