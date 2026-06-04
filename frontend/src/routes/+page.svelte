<script lang="ts">
import SearchForm from "$lib/components/SearchForm.svelte";
import StatCard from "$lib/components/StatCard.svelte";
import { getStatusContext } from "$lib/context/status";

const statusQuery = getStatusContext();
const status = $derived(statusQuery.data);
</script>

<svelte:head>
	<title>Cheburcheck</title>
</svelte:head>

<div class="mb-8">
	<h1 class="mb-2 text-2xl text-neutral-100 uppercase font-bold">
		Статус Ресурса
	</h1>
	<p class="text-neutral-500">
		Введите домен или IP-адрес для поиска по спискам заблокированных адресов и
		хостинг-провайдеров.
	</p>
</div>

<SearchForm />

<div
	class="mt-8 grid grid-cols-1 gap-4 text-xs text-neutral-500 sm:grid-cols-3"
>
	<StatCard
		icon="globe"
		label="Количество доменов"
		value={new Intl.NumberFormat('ru-RU').format(status?.domain_count ?? 0)}
	/>
	<StatCard
		icon="server"
		label="Количество IPv4-адресов"
		value={new Intl.NumberFormat('ru-RU').format(status?.v4_count ?? 0)}
	/>
	<StatCard
		icon="activity"
		label="Последнее обновление"
		value={new Intl.DateTimeFormat('ru-RU', {
			year: 'numeric',
			month: '2-digit',
			day: '2-digit',
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
		}).format(new Date(status?.last_update ?? 0))}
	/>
</div>
