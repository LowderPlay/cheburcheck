export function russianPlural(
	count: number,
	one: string,
	few: string,
	many: string,
): string {
	const value = Math.abs(Math.trunc(count));
	const lastTwo = value % 100;
	if (lastTwo >= 11 && lastTwo <= 14) return many;
	const lastDigit = value % 10;
	if (lastDigit === 1) return one;
	if (lastDigit >= 2 && lastDigit <= 4) return few;
	return many;
}

export const scannerWord = (count: number) =>
	russianPlural(count, "сканер", "сканера", "сканеров");

export const regionWord = (count: number) =>
	russianPlural(count, "регион", "региона", "регионов");

export const scannersAfterIzWord = (count: number) =>
	russianPlural(count, "сканера", "сканеров", "сканеров");

export function respondedScanners(count: number): string {
	return `${scannerWord(count) === "сканер" ? "Ответил" : "Ответили"} ${count} ${scannerWord(count)}`;
}
