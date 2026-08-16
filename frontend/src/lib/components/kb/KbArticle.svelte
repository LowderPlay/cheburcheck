<script lang="ts">
import type { Snippet } from "svelte";
import { getKbContext } from "$lib/context/kb.svelte";

let { children }: { children: Snippet } = $props();

const kb = getKbContext();
kb?.reset();
</script>

<article
	class="
		[&_a]:text-neutral-100 [&_a]:underline [&_a]:underline-offset-4 [&_a]:decoration-neutral-700 [&_a:hover]:decoration-neutral-400 [&_a]:transition-colors
		[&_code]:bg-neutral-900/70 [&_code]:px-1.5 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-sm [&_code]:text-neutral-200
		[&_h1]:mb-16 [&_h1]:text-5xl [&_h1]:font-black [&_h1]:tracking-tight [&_h1]:text-neutral-100 [&_h1]:uppercase
		[&_li]:my-3 [&_li]:text-neutral-300
		[&_ol]:list-decimal [&_ol]:pl-6 [&_ol]:my-6
		[&_p]:my-6 [&_p]:text-lg [&_p]:leading-relaxed [&_p]:text-neutral-300
		[&_ul]:list-disc [&_ul]:pl-6 [&_ul]:my-6
	"
>
	{@render children()}
</article>
