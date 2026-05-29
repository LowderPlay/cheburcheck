<script lang="ts">
import { CloudAlert, Copyright } from "@lucide/svelte";
import { createQuery } from "@tanstack/svelte-query";
import type { Snippet } from "svelte";
import { browser } from "$app/environment";
import type { StatusMetrics } from "$lib/api/status";
import { fetchStatus } from "$lib/api/status";
import { setStatusContext } from "$lib/context/status";
import GitHubIcon from "./GitHubIcon.svelte";

let { children }: { children: Snippet } = $props();
const year = new Date().getFullYear();
const statusCacheKey = "cheburcheck:status";

type CachedStatus = {
	data: StatusMetrics;
	updatedAt: number;
};

function getCachedStatus() {
	if (!browser) {
		return null;
	}

	const cachedStatus = localStorage.getItem(statusCacheKey);
	if (!cachedStatus) {
		return null;
	}

	try {
		return JSON.parse(cachedStatus) as CachedStatus;
	} catch {
		localStorage.removeItem(statusCacheKey);
		return null;
	}
}

const cachedStatus = getCachedStatus();
const status = createQuery(() => ({
	queryKey: ["status"],
	queryFn: () => fetchStatus(),
	initialData: cachedStatus?.data,
	initialDataUpdatedAt: cachedStatus?.updatedAt,
}));
setStatusContext(status);

$effect(() => {
	if (!browser || !status.data) {
		return;
	}

	localStorage.setItem(
		statusCacheKey,
		JSON.stringify({
			data: status.data,
			updatedAt: Date.now(),
		}),
	);
});
</script>

<header class="mx-auto mb-12 w-full max-w-250 border-b border-neutral-800 pb-4">
	<div class="flex items-center justify-between">
		<a
			class="flex items-center gap-2 text-neutral-100 no-underline tracking-tighter"
			href="/"
		>
			<CloudAlert size={32} aria-hidden="true" />
			<span class="text-2xl font-bold uppercase">Cheburcheck</span>
		</a>
		<div class="flex gap-4 text-xs text-neutral-500 items-center">
			<a href="/kb/faq" class="text-neutral-500 underline">FAQ</a>
			<a
				class="flex items-center gap-2 text-neutral-500 no-underline"
				href="https://github.com/LowderPlay/cheburcheck"
			>
				<span>v{status.data?.version}</span>
				<GitHubIcon />
			</a>
		</div>
	</div>
</header>

<main
	class="relative mx-auto flex w-full max-w-250 grow flex-col justify-center"
>
	{@render children()}
</main>

<footer
	class="mx-auto mt-12 w-full max-w-250 border-t border-neutral-800 pt-4 text-center text-[#525252]"
>
	<p class="inline-flex items-center gap-[0.35rem] text-xs uppercase">
		<a href="mailto:support@cheburcheck.ru" class="underline">
			support@cheburcheck.ru
		</a>
		- Сделано в России
		<Copyright size={10} aria-hidden="true" />
		{year}
	</p>
</footer>
