<script lang="ts">
import { LoaderCircle } from "@lucide/svelte";
import { createQuery, useQueryClient } from "@tanstack/svelte-query";
import { page } from "$app/state";
import { CheckRequestError, fetchCheck } from "$lib/api/check";
import {
	type ProbeResult,
	type ProbeStatus,
	startProbeSSE,
} from "$lib/api/probe";
import EmptyResult from "$lib/components/EmptyResult.svelte";
import ErrorMessage from "$lib/components/ErrorMessage.svelte";
import Feedback from "$lib/components/Feedback.svelte";
import ResultPanel from "$lib/components/ResultPanel.svelte";
import ProbeTable from "$lib/components/result/ProbeTable.svelte";
import SearchForm from "$lib/components/SearchForm.svelte";

const queryClient = useQueryClient();
const target = $derived(page.url.searchParams.get("target")?.trim() ?? "");

const checkQuery = createQuery(() => ({
	queryKey: ["check", target],
	queryFn: () => fetchCheck(target),
	enabled: target.length > 0,
	staleTime: Infinity,
}));

const queryId = $derived(checkQuery.data?.id);

const probeQuery = createQuery(() => ({
	queryKey: ["probes", queryId],
	queryFn: () => ({
		probes: [] as ProbeResult[],
		status: {
			status: "started",
			online_probes: 0,
			response_count: 0,
		} as ProbeStatus,
	}),
	enabled: !!queryId,
	staleTime: Infinity,
	gcTime: Infinity,
}));

$effect(() => {
	if (!queryId) return;

	queryClient.setQueryData(["probes", queryId], {
		probes: [],
		status: {
			id: queryId,
			target,
			status: "started",
			online_probes: 0,
			response_count: 0,
		},
	});

	const cleanup = startProbeSSE(
		queryId,
		(result) => {
			queryClient.setQueryData(["probes", queryId], (old: any) => ({
				...old,
				probes: [...(old?.probes || []), result],
			}));
		},
		(statusUpdate) => {
			queryClient.setQueryData(["probes", queryId], (old: any) => ({
				...old,
				status: { ...(old?.status || {}), ...statusUpdate },
			}));
		},
	);

	return cleanup;
});

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

	{#if probeQuery.data && checkQuery.data.targetType === 'Домен'}
		<ProbeTable
			probes={probeQuery.data.probes}
			status={probeQuery.data.status}
			isStaticBlocked={checkQuery.data.blocked}
		/>
	{/if}

	{#if checkQuery.data.id}
		<div class="mt-4">
			<Feedback id={checkQuery.data.id} />
		</div>
	{/if}
{/if}
