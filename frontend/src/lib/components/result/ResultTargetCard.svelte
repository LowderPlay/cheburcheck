<script lang="ts">
let {
	targetType,
	target,
}: {
	targetType: string;
	target: string;
} = $props();
</script>

<div class="mb-8 border border-neutral-800 bg-neutral-900/50 p-4">
	<div class="mb-1 text-xs text-neutral-500">{targetType}:</div>
	<div class="text-xl text-white">{target}</div>
</div>
