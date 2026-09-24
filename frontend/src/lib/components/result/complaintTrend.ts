import type { ComplaintDay } from "$lib/api/check";

const MIN_RECENT_COMPLAINTS = 3;
const MIN_COMPLAINT_INCREASE = 2;
const MIN_GROWTH_FACTOR = 1.5;

export function shouldShowComplaintTrend(days: ComplaintDay[]): boolean {
	if (days.length !== 14) return false;

	const previous = days.slice(0, 7).reduce((sum, day) => sum + day.count, 0);
	const recent = days.slice(7).reduce((sum, day) => sum + day.count, 0);

	return (
		recent >= MIN_RECENT_COMPLAINTS &&
		recent - previous >= MIN_COMPLAINT_INCREASE &&
		recent >= previous * MIN_GROWTH_FACTOR
	);
}
