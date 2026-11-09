<script lang="ts">
import type { CalibrationDebug } from "$lib/api/adminModels";
import TcpDiagnosticsView from "./TcpDiagnosticsView.svelte";

let { calibration }: { calibration: CalibrationDebug | undefined } = $props();
</script>

{#if calibration}
	<details class="my-3 rounded-lg border border-neutral-700 p-3 text-xs">
		<summary class="cursor-pointer text-neutral-300">
			Калибровка: {calibration.method}; DPI hop: {calibration.dpi_hop ?? "—"}
		</summary>
		{#if calibration.fallback_reason}
			<p class="mt-2">Причина fallback: {calibration.fallback_reason}</p>
		{/if}
		{#if calibration.failure_reason}
			<p class="mt-2 text-amber-300">
				Не подтверждено: {calibration.failure_reason}
			</p>
		{/if}
		{#if calibration.control_sni}
			<p class="mt-2">Контрольный SNI: {calibration.control_sni}</p>
		{/if}
		{#if calibration.first_reset_ttl !== undefined}
			<p class="mt-2">
				Последний подтверждённый TTL до фильтра:
				{calibration.last_clear_ttl ?? "—"}; первый TTL с RST:
				{calibration.first_reset_ttl}. TCP-проверки начинаются после верхней
				границы.
			</p>
		{/if}
		{#if calibration.control_witness_ttl !== undefined}
			<p class="mt-2">
				Контрольный ICMP за границей: TTL {calibration.control_witness_ttl}.
			</p>
		{/if}
		<ol class="mt-3 space-y-2">
			{#each calibration.attempts ?? [] as attempt}
				<li class="rounded border border-neutral-800 p-2">
					<span class="font-mono"
						>TTL {attempt.ttl} ·
						{attempt.control ? "контроль" : "проверяемый SNI"}
						·
						{attempt.outcome}
						· {attempt.router ?? "—"}</span
					>
					{#if attempt.error}
						<p class="text-amber-300">{attempt.error}</p>
					{/if}
					<TcpDiagnosticsView diagnostics={attempt.tcp_diagnostics} />
				</li>
			{/each}
		</ol>
	</details>
{/if}
