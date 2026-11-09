<script lang="ts">
import {
	Download,
	Radio,
	RefreshCw,
	Route,
	Settings2,
	Trash2,
} from "@lucide/svelte";
import { formatDate } from "$lib/admin/format";
import type { CommandType, Probe } from "$lib/api/adminModels";

let {
	probes,
	loading,
	busy,
	onedit,
	ontoggle,
	oncommand,
	onupdate,
	onremove,
	ontrace,
}: {
	probes: Probe[];
	loading: boolean;
	busy: string;
	onedit: (probe: Probe) => void;
	ontoggle: (
		probe: Probe,
		key: "hidden" | "disable_traceroutes" | "cdn_unblocked",
	) => Promise<void>;
	oncommand: (id: number, type: CommandType) => Promise<void>;
	onupdate: (id: number) => Promise<void>;
	onremove: (probe: Probe) => Promise<void>;
	ontrace: (id: number) => void;
} = $props();
let onlineOnly = $state(false);
const sortedProbes = $derived(
	[...probes].sort(
		(a, b) => Number(b.online) - Number(a.online) || a.id - b.id,
	),
);
const visibleProbes = $derived(
	onlineOnly ? sortedProbes.filter((probe) => probe.online) : sortedProbes,
);
function asnTarget(asn: string): string {
	const value = asn.trim().toUpperCase();
	return value.startsWith("AS") ? value : `AS${value}`;
}
</script>

<section class="panel overflow-x-auto">
	<div class="mb-4 flex flex-wrap items-center justify-between gap-3">
		<h2 class="text-lg font-semibold">Текущие сканеры</h2>
		<div class="flex flex-wrap items-center gap-4">
			<label class="check text-xs">
				<input type="checkbox" bind:checked={onlineOnly}>
				Только онлайн
			</label>
			<span class="text-xs text-neutral-500"
				>{probes.length}
				всего · {probes.filter((p) => p.online).length} онлайн</span
			>
		</div>
	</div>
	{#if visibleProbes.length === 0}
		<p class="py-8 text-center text-sm text-neutral-500">
			{loading ? "Загрузка…" : onlineOnly && probes.length > 0 ? "Нет сканеров онлайн" : "Сканеров пока нет"}
		</p>
	{:else}
		<table class="w-full min-w-[780px] text-left text-sm">
			<thead
				class="border-b border-neutral-800 text-xs uppercase tracking-wider text-neutral-500"
			>
				<tr>
					<th class="py-3 pr-3">Сканер</th>
					<th class="px-3">Состояние</th>
					<th class="px-3">Метаданные</th>
					<th class="px-3">Флаги</th>
					<th class="pl-3">Действия</th>
				</tr>
			</thead>
			<tbody>
				{#each visibleProbes as probe (probe.id)}
					<tr class="border-b border-neutral-800/70 align-top last:border-0">
						<td class="py-4 pr-3">
							<div class="font-semibold">#{probe.id}</div>
							<div class="mt-1 text-xs text-neutral-500">
								<p>{probe.name}</p>
								{probe.last_connected_at ? `${formatDate(probe.last_connected_at)}` : "Не подключался"}
							</div>
						</td>
						<td class="px-3 py-4">
							<span
								class:!text-emerald-300={probe.online}
								class="inline-flex items-center gap-1 text-neutral-500"
								><Radio size={14} />
								{probe.online ? "Онлайн" : "Офлайн"}</span
							>
							<div class="mt-1 text-xs text-neutral-500">
								{probe.version ?? "Версия неизвестна"}
								{probe.bundle_type ? `· ${probe.bundle_type}` : ""}
							</div>
							{#if probe.dpi_hop_v4 !== null || probe.dpi_hop_v6 !== null}
								<div class="mt-1 text-xs text-neutral-500">
									v4: <b>{probe.dpi_hop_v4 ?? "—"}</b>; v6:
									<b>{probe.dpi_hop_v6 ?? "—"}</b>
								</div>
							{/if}
						</td>
						<td class="px-3 py-4 text-xs text-neutral-400">
							{probe.region || "Регион не указан"}<br>
							{probe.provider || "Провайдер не указан"}
							{#if probe.asn}
								·
								<a
									class="text-cyan-300 underline hover:text-cyan-200"
									href={`/check?target=${encodeURIComponent(asnTarget(probe.asn))}`}
									>{probe.asn}</a
								>
							{/if}
						</td>
						<td class="px-3 py-4">
							<div class="flex flex-col items-start gap-1">
								<button
									type="button"
									class="flag"
									class:active={probe.hidden}
									disabled={busy !== ""}
									onclick={() => void ontoggle(probe, "hidden")}
								>
									{probe.hidden ? "Скрыт" : "Публичный"}
								</button><button
									type="button"
									class="flag"
									class:active={probe.disable_traceroutes}
									disabled={busy !== ""}
									onclick={() => void ontoggle(probe, "disable_traceroutes")}
								>
									{probe.disable_traceroutes ? "Трассировка выкл." : "Трассировка вкл."}
								</button><button
									type="button"
									class="flag"
									class:active={probe.cdn_unblocked}
									disabled={busy !== ""}
									onclick={() => void ontoggle(probe, "cdn_unblocked")}
								>
									{probe.cdn_unblocked ? "CDN доступен" : "CDN блокируется"}
								</button>
							</div>
						</td>
						<td class="pl-3 py-4">
							<div class="flex flex-col items-start gap-2">
								<button
									type="button"
									class="link"
									onclick={() => onedit(probe)}
								>
									<Settings2 size={14} />
									Изменить
								</button><button
									type="button"
									class="link"
									disabled={busy !== ""}
									onclick={() => void oncommand(probe.id, "resubscribe_tasks")}
								>
									<RefreshCw size={14} />
									Переподписать
								</button><button
									type="button"
									class="link"
									disabled={busy !== "" || !probe.online}
									onclick={() => void oncommand(probe.id, "remeasure_dpi_hop")}
								>
									<RefreshCw size={14} />
									Перемерить DPI hop
								</button><button
									type="button"
									class="link"
									disabled={busy !== ""}
									onclick={() => void onupdate(probe.id)}
								>
									<Download size={14} />
									Проверить обновление
								</button><button
									type="button"
									class="link"
									onclick={() => { ontrace(probe.id); }}
								>
									<Route size={14} />
									Traceroute
								</button>
								<button
									type="button"
									class="link remove-link"
									disabled={busy !== ""}
									onclick={() => void onremove(probe)}
								>
									<Trash2 size={14} />
									Удалить
								</button>
							</div>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

<style>
.flag {
	border: 1px solid #404040;
	border-radius: 5px;
	padding: 0.15rem 0.4rem;
	color: #a3a3a3;
	font-size: 0.7rem;
	cursor: pointer;
}
.flag.active {
	border-color: #0e7490;
	color: #67e8f9;
}
</style>
