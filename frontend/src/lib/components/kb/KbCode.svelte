<script lang="ts">
import { Check, Copy } from "@lucide/svelte";
import { onDestroy, type Snippet } from "svelte";

let {
	children,
	copyable = false,
}: {
	children: Snippet;
	copyable?: boolean;
} = $props();

let contentElement: HTMLSpanElement;
let copyState = $state<"idle" | "copied" | "failed">("idle");
let resetTimeout: ReturnType<typeof setTimeout> | undefined;

async function _copyCode() {
	try {
		await navigator.clipboard.writeText(
			contentElement.textContent?.trim() ?? "",
		);
		copyState = "copied";
	} catch {
		copyState = "failed";
	}

	if (resetTimeout) clearTimeout(resetTimeout);
	resetTimeout = setTimeout(() => {
		copyState = "idle";
	}, 2000);
}

onDestroy(() => {
	if (resetTimeout) clearTimeout(resetTimeout);
});
</script>

<pre
	class="relative my-8 overflow-x-auto rounded-lg border border-neutral-800 bg-neutral-900/50 p-6 font-mono text-sm leading-relaxed text-neutral-300 shadow-inner"
	class:pt-12={copyable}
><span bind:this={contentElement}>{@render children()}</span>{#if copyable}<button
		type="button"
		onclick={_copyCode}
		class="absolute top-2 right-2 inline-flex h-7 cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-md border border-neutral-700 bg-neutral-800 px-2 text-xs leading-none text-neutral-200 transition-colors hover:bg-neutral-700 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-neutral-300"
		aria-label={copyState === "copied"
			? "Скопировано"
			: copyState === "failed"
				? "Не удалось скопировать"
				: "Копировать код"}
		title={copyState === "copied"
			? "Скопировано"
			: copyState === "failed"
				? "Не удалось скопировать"
				: "Копировать код"}
	>
		{#if copyState === "copied"}
			<Check size={14} aria-hidden="true" />
			Скопировано
		{:else}
			<Copy size={14} aria-hidden="true" />
			{copyState === "failed" ? "Ошибка копирования" : "Копировать"}
		{/if}
	</button>{/if}</pre>
