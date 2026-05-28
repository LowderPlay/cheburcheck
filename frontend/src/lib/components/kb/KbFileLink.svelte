<script lang="ts">
import { FileSymlink } from "@lucide/svelte";
import type { Snippet } from "svelte";

let { children, href }: { children: Snippet; href: string } = $props();
</script>

<div class="my-1 flex items-center gap-2">
	<FileSymlink size={16} aria-hidden="true" />
	<a {href}>{@render children()}</a>
</div>
