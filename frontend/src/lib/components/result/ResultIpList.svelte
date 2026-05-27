<script lang="ts">
let {
	ips,
	subnetSize,
}: {
	ips: string[];
	subnetSize?: string | null;
} = $props();
</script>

<div class="flex w-full flex-col items-end gap-2">
	{#if ips.length === 1}
		<span class="break-all font-mono text-sm font-medium text-neutral-200">
			{ips[0]}
		</span>
		{#if subnetSize}
			<span class="text-xs text-neutral-500">({subnetSize} всего)</span>
		{/if}
	{:else if ips.length <= 5}
		{#each ips as ip}
			<span class="break-all font-mono text-sm font-medium text-neutral-200">
				{ip}
			</span>
		{/each}
		{#if subnetSize}
			<span class="text-xs text-neutral-500">({subnetSize} всего)</span>
		{/if}
	{:else}
		<div class="flex flex-col gap-1 text-right">
			<span class="font-mono text-sm font-medium text-neutral-200">
				{ips[0]}
				- {ips[ips.length - 1]}
			</span>
			{#if subnetSize}
				<span class="text-xs text-neutral-500">
					({subnetSize}
					всего, проверено {ips.length}
					)
				</span>
			{:else}
				<span class="text-xs text-neutral-500">({ips.length} адресов)</span>
			{/if}
		</div>
		<details class="w-fit">
			<summary
				class="w-fit max-w-37.5 cursor-pointer list-none border border-neutral-800 bg-neutral-900/50 px-2.5 py-1 text-center text-[0.7rem] text-neutral-500 select-none transition-all marker:hidden hover:border-neutral-700 hover:bg-neutral-800/50 hover:text-neutral-100 [&::-webkit-details-marker]:hidden"
			>
				Показать все
			</summary>
			<div
				class="mt-3 grid max-h-100 grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2 overflow-y-auto border border-neutral-800 bg-neutral-900/30 p-3"
			>
				{#each ips as ip}
					<span
						class="break-all font-mono text-sm font-medium text-neutral-200"
					>
						{ip}
					</span>
				{/each}
			</div>
		</details>
	{/if}
</div>
