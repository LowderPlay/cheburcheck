<script lang="ts">
import { page } from "$app/state";
import { setKbContext } from "$lib/context/kb.svelte";

let { children } = $props();

const kb = setKbContext();
</script>

<div class="flex flex-col gap-12 xl:flex-row xl:items-start">
	<aside class="order-1 xl:w-56 xl:shrink-0 xl:sticky xl:top-8">
		<nav
			aria-label="Table of contents"
			class="bg-neutral-900/20 p-6 rounded-xl border border-neutral-800 xl:bg-transparent xl:p-0 xl:rounded-none xl:border-0"
		>
			<h3
				class="mb-4 text-xs font-bold tracking-widest text-neutral-500 uppercase"
			>
				Содержание
			</h3>
			<ul class="flex flex-col gap-3 border-l border-neutral-800 pl-4">
				{#each kb.headings as heading}
					<li>
						<a
							href={`#${heading.id}`}
							class="block text-sm text-neutral-400 no-underline transition-colors hover:text-neutral-100"
						>
							{heading.title}
						</a>
					</li>
				{/each}
			</ul>
		</nav>
	</aside>

	<div class="order-2 flex-1 min-w-0 xl:max-w-3xl">
		{@render children()}
	</div>
</div>
