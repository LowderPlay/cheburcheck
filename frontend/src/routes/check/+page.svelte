<script lang="ts">
import { LoaderCircle } from "@lucide/svelte";
import { createQuery, useQueryClient } from "@tanstack/svelte-query";
import { page } from "$app/state";
import { CheckRequestError, fetchCheck } from "$lib/api/check";
import {
	type ProbeResult,
	type ProbeStatus,
	selectProbeVerdict,
	startProbeSSE,
} from "$lib/api/probe";
import EmptyResult from "$lib/components/EmptyResult.svelte";
import ErrorMessage from "$lib/components/ErrorMessage.svelte";
import Feedback from "$lib/components/Feedback.svelte";
import ResultPanel from "$lib/components/ResultPanel.svelte";
import ProbeTable from "$lib/components/result/ProbeTable.svelte";
import SearchForm from "$lib/components/SearchForm.svelte";

type ProbeQueryData = {
	probes: ProbeResult[];
	status: ProbeStatus;
};

const queryClient = useQueryClient();
const target = $derived(page.url.searchParams.get("target")?.trim() ?? "");

const checkQuery = createQuery(() => ({
	queryKey: ["check", target],
	queryFn: () => fetchCheck(target),
	enabled: target.length > 0,
	staleTime: Infinity,
}));

const queryId = $derived(checkQuery.data?.id);
const shouldProbe = $derived(!!queryId);

function createInitialProbeData(id: string): ProbeQueryData {
	return {
		probes: [],
		status: {
			id,
			target,
			status: "started",
			online_probes: 0,
			response_count: 0,
		},
	};
}

const probeQuery = createQuery(() => ({
	queryKey: ["probes", queryId],
	queryFn: () => createInitialProbeData(queryId ?? ""),
	enabled: shouldProbe,
	staleTime: Infinity,
	gcTime: Infinity,
}));

$effect(() => {
	if (!queryId || !shouldProbe) return;

	queryClient.setQueryData<ProbeQueryData>(
		["probes", queryId],
		createInitialProbeData(queryId),
	);

	const cleanup = startProbeSSE(
		queryId,
		(result) => {
			queryClient.setQueryData<ProbeQueryData>(["probes", queryId], (old) => {
				const current = old ?? createInitialProbeData(queryId);
				const probes = current.probes.some(
					(probe) => probe.probe_id === result.probe_id,
				)
					? current.probes.map((probe) =>
							probe.probe_id === result.probe_id ? result : probe,
						)
					: [...current.probes, result];

				return {
					...current,
					probes,
					status: {
						...current.status,
						status: "progress",
						response_count: probes.length,
					},
				};
			});
		},
		(statusUpdate) => {
			queryClient.setQueryData<ProbeQueryData>(["probes", queryId], (old) => {
				const current = old ?? createInitialProbeData(queryId);

				return {
					...current,
					status: { ...current.status, ...statusUpdate },
				};
			});
		},
	);

	return cleanup;
});

const error = $derived(
	(checkQuery.error instanceof CheckRequestError && checkQuery.error) || null,
);
const liveVerdict = $derived(
	checkQuery.data && probeQuery.data
		? selectProbeVerdict(probeQuery.data.probes, checkQuery.data.blocked)
		: null,
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
	<ResultPanel result={checkQuery.data} probeVerdict={liveVerdict} />

	{#if shouldProbe && probeQuery.data && probeQuery.data.status.online_probes > 0}
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
