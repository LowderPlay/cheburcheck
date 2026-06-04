<script lang="ts">
import type { Snippet } from "svelte";

let {
	children,
	height = "h-120",
}: {
	children: Snippet;
	height?: string;
} = $props();
</script>

<div
	class="{height} my-8 rounded-lg border border-neutral-800 bg-neutral-950/30 p-6 shadow-inner"
>
	{@render children()}
</div>
