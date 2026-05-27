<script lang="ts">
import { LoaderCircle } from "@lucide/svelte";
import { createQuery } from "@tanstack/svelte-query";
import { page } from "$app/state";
import { CheckRequestError, fetchCheck } from "$lib/api/check";
import EmptyResult from "$lib/components/EmptyResult.svelte";
import ErrorMessage from "$lib/components/ErrorMessage.svelte";
import ResultPanel from "$lib/components/ResultPanel.svelte";
import SearchForm from "$lib/components/SearchForm.svelte";

const target = $derived(page.url.searchParams.get("target")?.trim() ?? "");
const checkQuery = createQuery(() => ({
	queryKey: ["check", target],
	queryFn: () => fetchCheck(target),
	enabled: target.length > 0,
	staleTime: Infinity,
}));

const error = $derived(
	(checkQuery.error instanceof CheckRequestError && checkQuery.error) || null,
);
</script>

<SearchForm />

{#if target.length === 0}
	<div class="mt-4 border border-neutral-800 p-6 text-sm text-neutral-500">
		Введите домен, IP-адрес, подсеть или ASN для проверки.
	</div>
{:else if checkQuery.isPending}
	<LoaderCircle class="mx-auto mt-4 animate-spin text-neutral-400" />
{:else if error?.status === 404}
	<EmptyResult targetType="Запрос" {target} />
{:else if error}
	<div class="mt-4 border border-red-900/30 bg-red-950/10 p-6">
		<ErrorMessage status={error.status} reason={error.message} />
	</div>
{:else if checkQuery.data}
	<ResultPanel result={checkQuery.data} />
{/if}
