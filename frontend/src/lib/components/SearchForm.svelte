<script lang="ts">
import { ChevronRight, Search } from "@lucide/svelte";
</script>

<form
	class="group relative flex w-full flex-col gap-3 sm:flex-row sm:gap-0"
	action="/check"
>
	<!-- svelte-ignore a11y_autofocus -->
	<!-- biome-ignore-start lint/a11y/noAutofocus: using autofocus is acceptable here -->
	<input
		type="text"
		name="target"
		placeholder="example.com, 1.1.1.1, 1.0.0.0/24, AS13335"
		class="h-16 w-full border border-neutral-700 bg-neutral-900 py-4 pr-4 pl-12 font-[inherit] text-lg text-white outline-none transition-all focus:border-neutral-400 focus:bg-neutral-800 focus:[+div]:text-white sm:grow sm:pr-6"
		minlength="3"
		required
		autofocus
	>
	<!-- biome-ignore-end lint/a11y/noAutofocus: using autofocus is acceptable here -->
	<div
		class="pointer-events-none absolute top-8 left-4 flex -translate-y-1/2 justify-center text-neutral-500 transition-colors"
	>
		<Search size={20} aria-hidden="true" />
	</div>
	<button
		type="submit"
		class="flex h-16 w-full cursor-pointer items-center justify-center gap-2 border border-neutral-700 bg-neutral-800 px-6 font-[inherit] text-sm font-bold text-neutral-100 uppercase transition-colors hover:bg-neutral-700 group-focus-within:border-neutral-400 group-focus-within:bg-neutral-700 group-focus-within:hover:bg-neutral-600 sm:ml-[-1px] sm:w-auto sm:justify-between"
	>
		<span>Проверить</span>
		<ChevronRight size={16} aria-hidden="true" class="text-primary" />
	</button>
</form>
