<script lang="ts">
import { TriangleAlert } from "@lucide/svelte";
import type { Snippet } from "svelte";

let {
	children,
	variant = "warning",
}: {
	children: Snippet;
	variant?: "warning" | "info";
} = $props();

const variantStyle = {
	warning: "border-red-900/30 bg-[#450a0a]/20 text-red-400",
	info: "border-blue-900/30 bg-[#0a1045]/20 text-blue-400",
};
</script>

<p
	class="my-8 flex items-start gap-4 border-l-4 p-6 text-lg [&_a]:text-inherit [&_a]:font-bold {variantStyle[variant]}"
>
	<TriangleAlert size={24} aria-hidden="true" class="mt-1 shrink-0" />
	<span>{@render children()} </span>
</p>
