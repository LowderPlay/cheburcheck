<script lang="ts">
import type { Snippet } from "svelte";

let { children }: { children: Snippet } = $props();
</script>

<article
	class="[&_a]:text-neutral-100 [&_code]:bg-neutral-900/70 [&_code]:px-1 [&_code]:py-0.5 [&_h1]:my-4 [&_h1]:text-5xl [&_h1]:tracking-[0.1em] [&_h1]:text-neutral-100 [&_h1]:uppercase [&_li]:my-1 [&_p]:my-4 [&_p]:text-justify [&_ul]:list-inside"
>
	{@render children()}
</article>
