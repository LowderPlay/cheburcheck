<script lang="ts">
import SearchForm from "$lib/components/SearchForm.svelte";
import StatCard from "$lib/components/StatCard.svelte";
import { homeStats } from "$lib/data/mock";
</script>

<svelte:head>
	<title>Cheburcheck</title>
</svelte:head>

<div class="mb-8">
	<h1
		class="mb-2 text-2xl tracking-widest text-neutral-100 uppercase font-bold"
	>
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
		value={homeStats.domainCount}
	/>
	<StatCard
		icon="server"
		label="Количество IPv4-адресов"
		value={homeStats.v4Count}
	/>
	<StatCard
		icon="activity"
		label="Последнее обновление"
		value={homeStats.lastUpdate}
	/>
</div>
