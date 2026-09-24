<script lang="ts">
import { MessageCircle, X } from "@lucide/svelte";
import Feedback from "./Feedback.svelte";

let { id, target }: { id: string; target: string } = $props();
let expanded = $state(true);
</script>

<aside
	hidden={!expanded}
	class="fixed bottom-3 right-3 left-3 z-40 rounded-lg border border-neutral-700 bg-neutral-950 p-4 text-neutral-100 shadow-2xl sm:bottom-5 sm:left-auto sm:right-5 sm:w-80"
	aria-label="Отзыв о доступности ресурса"
>
	<div class="flex items-center justify-between gap-3">
		<span class="flex items-center gap-2 text-sm font-bold text-neutral-200">
			<MessageCircle size={17} aria-hidden="true" />
			Отзыв о доступности
		</span>
		<button
			type="button"
			class="cursor-pointer rounded p-1 text-neutral-400 transition-colors hover:bg-neutral-800 hover:text-white"
			aria-label="Свернуть окно отзыва"
			onclick={() => (expanded = false)}
		>
			<X size={18} aria-hidden="true" />
		</button>
	</div>
	<Feedback {id} {target} />
</aside>
{#if !expanded}
	<button
		type="button"
		class="fixed bottom-5 right-5 z-40 flex cursor-pointer items-center justify-center rounded-lg border border-neutral-700 bg-neutral-900 p-3 text-neutral-100 shadow-xl transition-colors hover:bg-neutral-800"
		aria-label="Открыть окно отзыва"
		onclick={() => (expanded = true)}
	>
		<MessageCircle size={20} aria-hidden="true" />
	</button>
{/if}
