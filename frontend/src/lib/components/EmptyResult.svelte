<script lang="ts">
import { ShieldQuestionMark } from "@lucide/svelte";

let { targetType, target }: { targetType: string; target: string } = $props();
</script>

<div class="mt-4 border border-neutral-800 p-6">
	<div class="mb-8 flex gap-4">
		<div
			class="flex items-center justify-center border border-current bg-white/5 p-3"
		>
			<ShieldQuestionMark size={32} aria-hidden="true" />
		</div>
		<div>
			<h2
				class="mb-1 text-3xl leading-[1.2] font-bold tracking-widest uppercase"
			>
				Не найдено
			</h2>
			<p class="text-sm">Возможно вы неправильно ввели запрос?</p>
		</div>
	</div>

	<div class="mb-8 border border-neutral-800 bg-neutral-900/50 p-4">
		<div class="mb-1 text-xs text-neutral-500">{targetType}:</div>
		<div class="text-xl text-white">{target}</div>
	</div>
</div>
