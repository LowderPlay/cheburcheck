<script lang="ts">
import { Copy, Plus, RefreshCw } from "@lucide/svelte";
import { onMount } from "svelte";
import { formatDate } from "$lib/admin/format";
import { createAdminPage } from "$lib/admin/page.svelte";
import type { ApiToken, IssuedApiToken } from "$lib/api/adminModels";
import AdminPage from "$lib/components/admin/AdminPage.svelte";

const admin = createAdminPage<ApiToken>("tokens", "/probe-api-keys", () => {
	issued = null;
});
let name = $state("");
let creditAmounts = $state<Record<string, number>>({});
let days = $state(90);
let noExpiry = $state(false);
let issued = $state<IssuedApiToken | null>(null);
let now = $state(Date.now());
const tokens = $derived(admin.query.data ?? []);
onMount(() => {
	const timer = setInterval(() => {
		now = Date.now();
	}, 30_000);
	return () => clearInterval(timer);
});

async function issueToken() {
	if (!name.trim() || admin.busy) return;
	issued = null;
	await admin.run(
		"issue",
		(request) =>
			request<IssuedApiToken>("/probe-api-keys", "POST", {
				name: name.trim(),
				expires_in_days: noExpiry ? null : days,
			}),
		async (token) => {
			issued = token;
			name = "";
			await admin.refresh();
		},
	);
}
async function revokeToken(token: ApiToken) {
	if (token.revoked_at) return;
	await admin.run(
		token.id,
		async (request) => {
			await admin.cancel();
			return request<ApiToken>(`/probe-api-keys/${token.id}/revoke`, "POST");
		},
		(revoked) => {
			admin.updateRows((rows) =>
				rows.map((row) => (row.id === revoked.id ? revoked : row)),
			);
			if (issued?.id === token.id) issued = null;
			admin.notice = `Токен «${token.name}» отозван`;
		},
	);
}
async function addCredits(token: ApiToken) {
	const credits = creditAmounts[token.id];
	if (!Number.isSafeInteger(credits) || credits <= 0 || admin.busy) return;
	await admin.run(
		`credits:${token.id}`,
		async (request) => {
			await admin.cancel();
			return request<ApiToken>(`/probe-api-keys/${token.id}/credits`, "POST", {
				credits,
			});
		},
		(updated) => {
			admin.updateRows((rows) =>
				rows.map((row) => (row.id === updated.id ? updated : row)),
			);
			delete creditAmounts[token.id];
			admin.notice = `Добавлено ${credits} кредитов для «${token.name}»`;
		},
	);
}
async function copyToken() {
	if (issued) await admin.copy(issued.token, "Токен скопирован");
}
</script>

<AdminPage title="API-токены" {admin}>
	<section class="panel overflow-x-auto">
		<div class="mb-4 flex flex-wrap items-center justify-between gap-3">
			<h2 class="text-lg font-semibold">Выпущенные токены</h2>
			<button
				type="button"
				class="btn"
				disabled={admin.query.isFetching || !!admin.busy}
				onclick={() => void admin.query.refetch()}
			>
				<RefreshCw size={16} />Обновить список
			</button>
		</div>
		{#if admin.query.isPending}
			<p class="py-8 text-center text-sm text-neutral-500">Загрузка токенов…</p>
		{:else if tokens.length === 0 && !admin.query.isError}
			<p class="py-8 text-center text-sm text-neutral-500">
				Токены пока не выпущены.
			</p>
		{:else if tokens.length > 0}
			<table class="w-full min-w-[740px] text-left text-sm">
				<thead class="border-b border-neutral-700 text-xs text-neutral-500">
					<tr>
						<th class="py-3 pr-3">Имя</th>
						<th class="px-3">Выпущен</th>
						<th class="px-3">Действует до</th>
						<th class="px-3">Статус</th>
						<th class="px-3">Кредиты</th>
						<th class="pl-3">Действие</th>
					</tr>
				</thead>
				<tbody>
					{#each tokens as token (token.id)}
						{@const expired =
                            token.expires_at !== null &&
                            new Date(token.expires_at).getTime() <= now}
						<tr class="border-b border-neutral-800">
							<td class="py-4 pr-3">
								<div class="font-semibold">{token.name}</div>
								<div class="mt-1 font-mono text-xs text-neutral-500">
									{token.id}
								</div>
							</td>
							<td class="px-3 py-4 text-neutral-400">
								{formatDate(token.created_at)}
							</td>
							<td class="px-3 py-4 text-neutral-400">
								{token.expires_at
                                    ? formatDate(token.expires_at)
                                    : "Бессрочно"}
							</td>
							<td class="px-3 py-4">
								<span
									class={token.revoked_at
                                        ? "text-red-300"
                                        : expired
                                          ? "text-amber-300"
                                          : "text-emerald-300"}
									>{token.revoked_at
                                        ? "Отозван"
                                        : expired
                                          ? "Истёк"
                                          : "Активен"}</span
								>
								{#if token.revoked_at}
									<div class="mt-1 text-xs text-neutral-500">
										{formatDate(token.revoked_at)}
									</div>
								{/if}
							</td>
							<td class="px-3 py-4">
								<div class="mb-2 font-semibold">
									{token.credits.toLocaleString("ru-RU")}
								</div>
								<form
									class="flex gap-2"
									onsubmit={(event) => {
                                        event.preventDefault();
                                        void addCredits(token);
                                    }}
								>
									<input
										class="input w-28"
										type="number"
										min="1"
										max={Number.MAX_SAFE_INTEGER -
                                            token.credits}
										step="1"
										required
										aria-label={`Добавить кредиты для ${token.name}`}
										placeholder="Кредиты"
										bind:value={creditAmounts[token.id]}
										disabled={!!admin.busy ||
                                            !!token.revoked_at ||
                                            expired}
									>
									<button
										class="btn"
										type="submit"
										disabled={!!admin.busy ||
                                            !!token.revoked_at ||
                                            expired}
									>
										Добавить
									</button>
								</form>
							</td>
							<td class="pl-3 py-4">
								<button
									type="button"
									class="btn text-red-300"
									disabled={!!admin.busy ||
                                        token.revoked_at !== null}
									onclick={() => void revokeToken(token)}
								>
									{admin.busy === token.id
                                        ? "Отзыв…"
                                        : token.revoked_at
                                          ? "Отозван"
                                          : "Отозвать"}
								</button>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</section>

	<section class="panel">
		<h2 class="mb-2 text-lg font-semibold">Выпустить API-токен</h2>
		<p class="mb-5 text-sm text-neutral-400">
			Токен позволяет запускать сетевые сканеры без проверки безопасности.
		</p>
		<form
			class="space-y-4"
			onsubmit={(event) => {
                event.preventDefault();
                void issueToken();
            }}
		>
			<div class="grid gap-4 sm:grid-cols-2">
				<label class="field"
					>Имя токена<input
						class="input"
						bind:value={name}
						maxlength="255"
						placeholder="Мониторинг"
						required
					></label
				>
				<label class="field"
					>Срок действия, дней<input
						class="input"
						type="number"
						bind:value={days}
						min="1"
						max="3650"
						step="1"
						disabled={noExpiry}
						required={!noExpiry}
					></label
				>
			</div>
			<label class="flex items-center gap-2 text-sm"
				><input type="checkbox" bind:checked={noExpiry}>Без срока действия</label
			>
			<button class="btn-primary" type="submit" disabled={!!admin.busy}>
				<Plus size={16} />
				{admin.busy === "issue" ? "Выпуск…" : "Выпустить токен"}
			</button>
		</form>
		{#if issued}
			<div class="mt-5 space-y-3 rounded-lg border border-emerald-800 p-4">
				<p class="font-semibold text-emerald-300">
					Токен «{issued.name}» выпущен
				</p>
				<p class="text-xs text-neutral-400">
					Сохраните токен сейчас: после закрытия его нельзя будет посмотреть
					повторно.
				</p>
				<div class="flex items-start gap-3">
					<pre
						class="min-w-0 flex-1 whitespace-pre-wrap break-all rounded bg-neutral-950 p-3 text-xs"
					>{issued.token}</pre>
					<button
						type="button"
						class="btn"
						aria-label="Скопировать API-токен"
						onclick={() => void copyToken()}
					>
						<Copy size={17} />
					</button>
				</div>
				<p class="text-xs text-neutral-400">
					{issued.expires_at
                        ? `Действует до ${formatDate(issued.expires_at)}`
                        : "Без срока действия"}
				</p>
				<p class="text-xs text-neutral-400">
					Передавайте в заголовке
					<code>Authorization: Bearer &lt;токен&gt;</code>.
				</p>
				<button
					type="button"
					class="btn"
					onclick={() => {
                        issued = null;
                    }}
				>
					Скрыть токен
				</button>
			</div>
		{/if}
	</section>
</AdminPage>
