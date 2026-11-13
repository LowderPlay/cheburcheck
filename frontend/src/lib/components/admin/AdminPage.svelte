<script lang="ts">
import type { Snippet } from "svelte";
import type { AdminPageState } from "$lib/admin/page.svelte";

let {
	title,
	admin,
	children,
}: {
	title: string;
	admin: AdminPageState;
	children: Snippet;
} = $props();
const pages = [
	{ href: "/admin/probes", title: "Сканеры" },
	{ href: "/admin/tokens", title: "API-токены" },
];
</script>

<svelte:head>
	<title>{title} · Cheburcheck</title>
	<meta name="robots" content="noindex, nofollow">
</svelte:head>

<div class="space-y-7 text-neutral-100">
	<div class="flex flex-wrap items-end justify-between gap-4">
		<h1 class="mt-2 text-3xl font-bold tracking-tight">{title}</h1>
		<nav aria-label="Администрирование" class="flex gap-4">
			{#each pages.filter((page) => page.title !== title) as page (page.href)}
				<a href={page.href} class="text-sm text-cyan-300 underline"
					>{page.title}</a
				>
			{/each}
		</nav>
		{#if admin.password}
			<button type="button" class="btn" onclick={admin.signOut}>Выйти</button>
		{/if}
	</div>
	{#if admin.loginRequired}
		<form
			class="panel max-w-md space-y-4"
			onsubmit={(event) => { event.preventDefault(); void admin.signIn(); }}
		>
			<label class="field" for="admin-password"
				>Пароль администратора
				<input
					id="admin-password"
					class="input"
					type="password"
					autocomplete="current-password"
					bind:value={admin.enteredPassword}
					required
				>
			</label>
			{#if admin.displayedError}
				<p role="alert" class="text-sm text-red-300">{admin.displayedError}</p>
			{/if}
			<button type="submit" class="btn-primary" disabled={admin.signingIn}>
				{admin.signingIn ? "Проверка…" : "Открыть"}
			</button>
		</form>
	{:else}
		{#if admin.notice}
			<div
				role="status"
				class="rounded-lg border border-emerald-800 bg-emerald-950/20 p-4 text-sm text-emerald-300"
			>
				{admin.notice}
			</div>
		{/if}
		{#if admin.displayedError}
			<div
				role="alert"
				class="rounded-lg border border-red-900 bg-red-950/20 p-4 text-sm text-red-300"
			>
				{admin.displayedError}
			</div>
		{/if}
		{@render children()}
	{/if}
</div>
