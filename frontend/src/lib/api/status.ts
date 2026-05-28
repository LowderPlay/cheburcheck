export type StatusMetrics = {
	domain_count: number;
	v4_count: number;
	last_update: string | null;
	version: string;
};

export class StatusRequestError extends Error {
	constructor(
		message: string,
		readonly status: number,
	) {
		super(message);
		this.name = "StatusRequestError";
	}
}

export async function fetchStatus(): Promise<StatusMetrics> {
	const response = await fetch(`/api/v1/status`, {
		headers: {
			Accept: "application/json",
		},
	});

	if (!response.ok) {
		throw new StatusRequestError(response.statusText, response.status);
	}

	return await response.json();
}
