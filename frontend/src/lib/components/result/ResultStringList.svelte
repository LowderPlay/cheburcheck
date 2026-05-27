<script lang="ts">
let {
	items,
	limit = 10,
	alert = false,
}: {
	items: string[];
	limit?: number;
	alert?: boolean;
} = $props();

const valueClass = $derived(
	`text-right text-sm font-medium text-neutral-200${alert ? " text-red-500" : ""}`,
);
</script>

<div class="flex w-full flex-col items-end gap-2">
	<div>
		{#each items.slice(0, limit) as item}
			<p class={valueClass}>{item}</p>
		{/each}
	</div>
	{#if items.length > limit}
		<details class="w-fit">
			<summary
				class="w-fit max-w-37.5 cursor-pointer list-none border border-neutral-800 bg-neutral-900/50 px-2.5 py-1 text-center text-[0.7rem] text-neutral-500 select-none transition-all marker:hidden hover:border-neutral-700 hover:bg-neutral-800/50 hover:text-neutral-100 [&::-webkit-details-marker]:hidden"
			>
				Показать все ({items.length})
			</summary>
			<div
				class="mt-3 flex max-h-100 flex-col gap-1 overflow-y-auto border border-neutral-800 bg-neutral-900/30 p-3"
			>
				{#each items as item}
					<p class={valueClass}>{item}</p>
				{/each}
			</div>
		</details>
	{/if}
</div>
