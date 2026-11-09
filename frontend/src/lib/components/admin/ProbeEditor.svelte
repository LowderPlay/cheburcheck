<script lang="ts">
import { Plus } from "@lucide/svelte";
import type { ProbeForm } from "$lib/api/adminModels";

let {
	form = $bindable(),
	editing,
	busy,
	onsave,
	oncancel,
}: {
	form: ProbeForm;
	editing: number | null;
	busy: string;
	onsave: () => void | Promise<void>;
	oncancel: () => void;
} = $props();
</script>

<section id="probe-form" class="panel">
	<h2 class="mb-5 text-lg font-semibold">
		{editing === null ? "Добавить сканер" : `Изменить сканер #${editing}`}
	</h2>
	<form
		class="space-y-4"
		onsubmit={(event) => { event.preventDefault(); void onsave(); }}
	>
		<div class="grid gap-4 sm:grid-cols-2">
			<label class="field"
				>Имя<input
					class="input"
					bind:value={form.name}
					maxlength="255"
					required
				></label
			><label class="field"
				>Регион<input
					class="input"
					bind:value={form.region}
					maxlength="255"
				></label
			><label class="field"
				>Провайдер<input
					class="input"
					bind:value={form.provider}
					maxlength="255"
				></label
			><label class="field"
				>ASN<input class="input" bind:value={form.asn} maxlength="32"></label
			>
		</div>
		<div class="flex flex-wrap gap-5 text-sm">
			<label class="check"
				><input type="checkbox" bind:checked={form.hidden}>
				Скрыт</label
			><label class="check"
				><input type="checkbox" bind:checked={form.disable_traceroutes}>
				Отключить трассировки</label
			><label class="check"
				><input type="checkbox" bind:checked={form.cdn_unblocked}>
				CDN доступен</label
			>
		</div>
		<div class="flex gap-3">
			<button
				type="submit"
				class="btn-primary flex items-center gap-2"
				disabled={busy !== ""}
			>
				<Plus size={16} />{editing === null ? "Создать" : "Сохранить"}
			</button>
			{#if editing !== null}
				<button type="button" class="btn" onclick={() => { oncancel(); }}>
					Отмена
				</button>
			{/if}
		</div>
	</form>
</section>
