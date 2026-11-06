import type { CheckResult } from "./check";
import type { ProbeResult, ProbeStatus } from "./probe";

export type SharedSnapshot = {
	version: 1;
	result: Omit<CheckResult, "id">;
	probes: ProbeResult[];
	probe_status: Pick<ProbeStatus, "online_probes" | "response_count" | "status"> | null;
};

export type SharedResult = {
	id: string;
	created_at: string;
	expires_at: string;
	snapshot: SharedSnapshot;
};

export async function createSharedResult(
	snapshot: SharedSnapshot,
): Promise<Pick<SharedResult, "id" | "created_at" | "expires_at">> {
	const response = await fetch("/api/v1/shares", {
		method: "POST",
		headers: {
			Accept: "application/json",
			"Content-Type": "application/json",
		},
		body: JSON.stringify(snapshot),
	});

	if (!response.ok) {
		throw new Error(
			response.status === 413
				? "Снимок слишком большой для создания ссылки"
				: response.status === 429
					? "Слишком много запросов. Попробуйте создать ссылку позже"
					: `Не удалось создать ссылку (ошибка ${response.status})`,
		);
	}

	return response.json();
}

export async function fetchSharedResult(
	id: string,
	fetcher: typeof fetch = fetch,
	signal?: AbortSignal,
): Promise<SharedResult> {
	const response = await fetcher(`/api/v1/shares/${encodeURIComponent(id)}`, {
		headers: { Accept: "application/json" },
		signal,
	});

	if (!response.ok) {
		throw new Error(
			response.status === 404
				? "Ссылка не найдена или срок её действия истёк"
				: `Не удалось загрузить результат (ошибка ${response.status})`,
		);
	}

	return response.json();
}
