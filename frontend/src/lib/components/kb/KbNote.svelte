<script lang="ts">
import { TriangleAlert } from "@lucide/svelte";
import type { Snippet } from "svelte";

let {
	children,
	variant = "warning",
}: {
	children: Snippet;
	variant?: "warning";
} = $props();
</script>

<p
	class="flex items-center gap-4 border p-4 text-xl [&_a]:text-inherit {variant ===
	'warning'
		? 'border-red-900/30 bg-[#450a0a]/20 text-red-400'
		: ''}"
>
	<TriangleAlert size={32} aria-hidden="true" />
	<span>{@render children()} </span>
</p>
