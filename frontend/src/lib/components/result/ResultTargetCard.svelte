<script lang="ts">
let {
	targetType,
	target,
	asnStats,
}: {
	targetType: string;
	target: string;
	asnStats?: { total: number; blocked: number };
} = $props();
</script>

<div
	class="border border-neutral-800 bg-neutral-900/50 px-5 py-4 rounded-lg flex flex-col justify-between h-full min-h-[100px]"
>
	<div class={asnStats ? "mb-4" : "my-auto"}>
		<div class="text-xs text-neutral-500 uppercase font-bold mb-1">
			{targetType}
		</div>
		<div class="text-xl text-white font-medium break-all">{target}</div>
	</div>

	{#if asnStats}
		<div class="pt-4 border-t border-neutral-800 grid grid-cols-2 gap-4">
			<div>
				<div class="text-[10px] text-neutral-500 uppercase font-bold mb-0.5">
					Всего подсетей
				</div>
				<div class="text-lg font-bold text-white">{asnStats.total}</div>
			</div>
			<div>
				<div
					class="text-[10px] uppercase font-bold mb-0.5 {asnStats.blocked > 0 ? 'text-red-500/70' : 'text-green-500/70'}"
				>
					Заблокировано
				</div>
				<div
					class="text-lg font-bold {asnStats.blocked > 0 ? 'text-red-500' : 'text-green-500'}"
				>
					{asnStats.blocked}
				</div>
			</div>
		</div>
	{/if}
</div>
