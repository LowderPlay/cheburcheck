<script lang="ts">
import "./layout.css";
import { QueryClient, QueryClientProvider } from "@tanstack/svelte-query";
import AppShell from "$lib/components/AppShell.svelte";

let { children } = $props();

const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			retry: false,
			staleTime: 30_000,
		},
	},
});
</script>

<svelte:head>
	<title>Cheburcheck</title>
	<meta
		name="description"
		content="Проверка доменов и IP-адресов на наличие в реестре блокировок Роскомнадзора и заблокированных CDN."
	>
	<meta
		name="keywords"
		content="проверка домена, заблокирован ли сайт, Роскомнадзор, чебурнет, cdn, блокировки"
	>
	<meta property="og:title" content="Cheburcheck">
	<meta
		property="og:description"
		content="Проверка доменов и IP-адресов на наличие в реестре блокировок Роскомнадзора и заблокированных CDN."
	>
	<meta property="og:url" content="https://cheburcheck.ru">
	<meta property="og:image" content="https://cheburcheck.ru/og-cover.png">
	<meta property="og:site_name" content="Cheburcheck">
	<meta property="og:type" content="website">
	<meta name="author" content="Cheburcheck">
	<link rel="icon" type="image/png" href="/favicon.png">
	<link rel="icon" type="image/svg+xml" href="/favicon.svg">
</svelte:head>

<QueryClientProvider client={queryClient}>
	<AppShell> {@render children()} </AppShell>
</QueryClientProvider>
