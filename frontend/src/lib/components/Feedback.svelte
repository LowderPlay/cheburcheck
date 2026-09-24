<script lang="ts">
import { ThumbsDown, ThumbsUp } from "@lucide/svelte";
import { createMutation } from "@tanstack/svelte-query";
import { submitFeedback } from "$lib/api/feedback";

let { id, target }: { id: string; target: string } = $props();

const feedbackMutation = createMutation(() => ({
	mutationFn: submitFeedback,
}));

const submit = (works: boolean) => {
	feedbackMutation.mutate({ id, works });
};
</script>

<div class="mt-2">
	{#if feedbackMutation.isSuccess}
		<div class="flex items-center gap-2 py-2 text-lg text-green-500">
			<ThumbsUp size={24} aria-hidden="true" />
			<span>Спасибо за отзыв!</span>
		</div>
	{:else}
		<p class="mb-3 text-sm text-neutral-500">
			У вас работает ресурс <strong>{target}</strong>?
		</p>
		<div class="flex gap-3">
			<button
				class="flex grow cursor-pointer items-center justify-center gap-2 px-4 py-2 font-[inherit] text-sm font-bold transition-all border border-green-500 bg-transparent text-green-500 hover:bg-green-500/10"
				type="button"
				disabled={feedbackMutation.isPending}
				onclick={() => submit(true)}
			>
				<ThumbsUp size={16} aria-hidden="true" />
				Работает
			</button>
			<button
				class="flex grow cursor-pointer items-center justify-center gap-2 px-4 py-2 font-[inherit] text-sm font-bold transition-all border border-red-500 bg-transparent text-red-500 hover:bg-red-500/10"
				type="button"
				disabled={feedbackMutation.isPending}
				onclick={() => submit(false)}
			>
				<ThumbsDown size={16} aria-hidden="true" />
				Не работает
			</button>
		</div>
		{#if feedbackMutation.isError}
			<p class="mt-3 text-sm text-red-400">
				Не удалось отправить отзыв. Попробуйте еще раз.
			</p>
		{/if}
	{/if}
</div>
