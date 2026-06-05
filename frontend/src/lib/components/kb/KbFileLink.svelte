<script lang="ts">
import { FileSymlink } from "@lucide/svelte";
import type { Snippet } from "svelte";

let { children, href }: { children: Snippet; href: string } = $props();
</script>

<div class="my-3 flex items-center gap-3">
	<div class="text-neutral-500">
		<FileSymlink size={20} aria-hidden="true" />
	</div>
	<a {href} class="text-lg font-medium">{@render children()}</a>
</div>
