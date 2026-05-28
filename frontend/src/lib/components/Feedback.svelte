<script lang="ts">
import { ThumbsDown, ThumbsUp } from "@lucide/svelte";
import { createMutation } from "@tanstack/svelte-query";
import { submitFeedback } from "$lib/api/feedback";

let {
	id,
	theme = "clean",
}: {
	id: string;
	theme?: "blocked" | "clean" | "whitelist";
} = $props();

const feedbackMutation = createMutation(() => ({
	mutationFn: submitFeedback,
}));

const worksClass = $derived(
	theme === "blocked"
		? "border-transparent bg-green-500 text-neutral-950 hover:bg-emerald-400"
		: "border border-green-500 bg-transparent text-green-500 hover:bg-green-500/10",
);
const notWorksClass = $derived(
	theme === "clean"
		? "border-transparent bg-red-500 text-neutral-100 hover:bg-red-400"
		: "border border-red-500 bg-transparent text-red-500 hover:bg-red-500/10",
);
const statusClass = $derived(
	theme === "blocked"
		? "text-red-400"
		: theme === "clean"
			? "text-green-500"
			: "text-neutral-300",
);
const submit = (works: boolean) => {
	feedbackMutation.mutate({ id, works });
};
</script>

<div class="mt-6 border-t border-dashed border-neutral-800 pt-4">
	{#if feedbackMutation.isSuccess}
		<div class={`flex items-center gap-2 py-2 text-sm ${statusClass}`}>
			<ThumbsUp size={16} aria-hidden="true" />
			<span>Спасибо за ваш отзыв!</span>
		</div>
	{:else}
		<p class="mb-3 text-sm text-neutral-500">У вас работает этот ресурс?</p>
		<div class="flex gap-3">
			<button
				class={`flex grow cursor-pointer items-center justify-center gap-2 border px-4 py-2 font-[inherit] text-sm font-bold transition-all ${worksClass}`}
				type="button"
				disabled={feedbackMutation.isPending}
				onclick={() => submit(true)}
			>
				<ThumbsUp size={16} aria-hidden="true" />
				Работает
			</button>
			<button
				class={`flex grow cursor-pointer items-center justify-center gap-2 border px-4 py-2 font-[inherit] text-sm font-bold transition-all ${notWorksClass}`}
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
