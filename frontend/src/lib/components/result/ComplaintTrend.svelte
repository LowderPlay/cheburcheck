<script lang="ts">
import { ChartNoAxesCombined } from "@lucide/svelte";
import { BarChart } from "layerchart";
import type { ComplaintDay } from "$lib/api/check";

let { days }: { days: ComplaintDay[] } = $props();

const previousCount = $derived(
	days.slice(0, 7).reduce((sum, day) => sum + day.count, 0),
);
const recentCount = $derived(
	days.slice(7).reduce((sum, day) => sum + day.count, 0),
);
const chartData = $derived(
	days.map((day) => ({
		label: `${day.date.slice(8, 10)}.${day.date.slice(5, 7)}`,
		count: day.count,
	})),
);
const series = [
	{ key: "count", label: "Жалобы", value: "count", color: "#ef4444" },
];
const chartProps = {
	xAxis: {
		tickSpacing: 28,
		tickLabelProps: { rotate: -35, textAnchor: "end", dx: -5, dy: 8 },
	},
} as const;
</script>

<section
	class="h-full rounded-lg border border-red-900/30 bg-red-950/10 p-4"
	aria-label="Рост числа жалоб"
>
	<div class="flex items-start gap-2">
		<ChartNoAxesCombined
			size={20}
			class="shrink-0 text-red-400"
			aria-hidden="true"
		/>
		<div>
			<h3 class="font-bold text-red-400">Жалоб стало больше</h3>
			<p class="text-sm text-neutral-300">
				За последние 7 дней: {recentCount}, за предыдущие: {previousCount}.
			</p>
		</div>
	</div>
	<div
		class="mt-2 h-28 min-w-0"
		role="img"
		aria-label="Количество жалоб по дням за последние 14 дней"
	>
		<BarChart
			data={chartData}
			x="label"
			y="count"
			{series}
			padding={{ top: 4, right: 8, bottom: 36, left: 32 }}
			props={chartProps}
		/>
	</div>
</section>
