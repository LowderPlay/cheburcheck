<script lang="ts">
import type { TcpDiagnostics } from "$lib/api/adminModels";

let { diagnostics }: { diagnostics: TcpDiagnostics | undefined } = $props();
</script>

{#if diagnostics}
	<details
		class="mt-2 rounded-lg border border-neutral-700 p-3 text-xs"
		open={diagnostics.packets.some((packet) => packet.flags.includes("RST") || packet.flags.includes("FIN"))}
	>
		<summary class="cursor-pointer text-neutral-300">
			TCP: {diagnostics.packets.length} входящих пакетов{diagnostics.truncated ? " (список сокращён)" : ""}
		</summary>
		{#if diagnostics.capture_error}
			<p class="mt-2 text-amber-300">
				Ошибка захвата: {diagnostics.capture_error}
			</p>
		{/if}
		{#if diagnostics.connect_error}
			<p class="mt-2 text-amber-300">
				Ошибка соединения: {diagnostics.connect_error}
			</p>
		{/if}
		{#if diagnostics.send_error}
			<p class="mt-2 text-amber-300">
				Ошибка отправки: {diagnostics.send_error}
			</p>
		{/if}
		{#if diagnostics.payload_sent === false}
			<p class="mt-2 text-amber-300">
				TTL-данные не записаны полностью{diagnostics.peer_closed_before_payload ? ": соединение закрыто до попытки отправки" : ""}.
			</p>
		{/if}
		<p class="mt-2 text-neutral-400">
			Время приблизительное, от начала захвата.
			{#if diagnostics.client_hello_sent_ms !== null}
				Попытка ClientHello: {diagnostics.client_hello_sent_ms} мс; попытка
				записи данных:
				{diagnostics.junk_sent_ms ?? "—"}
				мс.
			{/if}
			Δ TTL сравнивает пакет с SYN-ACK того же соединения; это не расстояние до
			DPI.
		</p>
		<div class="mt-3 overflow-x-auto">
			<table class="w-full whitespace-nowrap text-left font-mono">
				<thead class="text-neutral-500">
					<tr>
						<th class="pr-4">мс</th>
						<th class="pr-4">Источник → цель</th>
						<th class="pr-4">Флаги</th>
						<th class="pr-4">TTL / Δ</th>
						<th class="pr-4">SEQ / ACK</th>
						<th class="pr-4">Окно / IP ID</th>
						<th class="pr-4">TS / echo</th>
						<th>Байты / опции</th>
					</tr>
				</thead>
				<tbody>
					{#each diagnostics.packets as packet}
						{@const baseline = diagnostics.packets.find((p) => p.destination === packet.destination && p.flags.includes("SYN") && p.flags.includes("ACK"))}
						<tr
							class="border-t border-neutral-800"
							class:text-amber-300={packet.flags.includes("RST") || packet.flags.includes("FIN")}
						>
							<td class="py-2 pr-4">{packet.observed_ms}</td>
							<td class="pr-4">{packet.source} → {packet.destination}</td>
							<td class="pr-4">{packet.flags.join(" ") || "—"}</td>
							<td class="pr-4">
								{packet.ttl ?? "—"}
								/
								{packet.ttl !== null && baseline?.ttl != null ? packet.ttl - baseline.ttl : "—"}
							</td>
							<td class="pr-4">{packet.sequence}/ {packet.acknowledgment}</td>
							<td class="pr-4">{packet.window} / {packet.ip_id ?? "—"}</td>
							<td class="pr-4">
								{packet.timestamp ?? "—"}
								/ {packet.timestamp_echo ?? "—"}
							</td>
							<td>{packet.payload_bytes} / {packet.options_hex || "—"}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		{#if diagnostics.packets.some((p) => p.ttl === null)}
			<p class="mt-2 text-neutral-500">
				Для части пакетов IP-заголовок недоступен; TTL не измерен.
			</p>
		{/if}
	</details>
{/if}
