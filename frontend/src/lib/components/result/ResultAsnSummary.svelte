<script lang="ts">
let {
	total,
	blocked,
}: {
	total: number;
	blocked: number;
} = $props();
</script>

<div
	class="mb-8 flex flex-wrap gap-8 border border-neutral-800 bg-neutral-900/50 p-4"
>
	<div class="flex flex-col gap-1">
		<span class="text-xs tracking-wider text-neutral-500 uppercase">
			Всего подсетей:
		</span>
		<span class="text-2xl font-bold text-neutral-100">{total}</span>
	</div>
	{#if blocked > 0}
		<div class="flex flex-col gap-1">
			<span class="text-xs tracking-wider text-neutral-500 uppercase">
				Заблокировано:
			</span>
			<span class="text-2xl font-bold text-red-500">{blocked}</span>
		</div>
	{/if}
</div>
