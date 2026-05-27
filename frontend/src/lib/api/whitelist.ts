type ApiWhitelistHistogramBin = {
	bin_min_rank?: number | null;
	bin_max_rank?: number | null;
	count?: number | null;
};

export type WhitelistHistogramBin = {
	label: string;
	count: number;
};

export class WhitelistHistogramRequestError extends Error {
	constructor(
		message: string,
		readonly status: number,
	) {
		super(message);
		this.name = "WhitelistHistogramRequestError";
	}
}

export async function fetchWhitelistHistogram(
	limit: number,
	filter = false,
): Promise<WhitelistHistogramBin[]> {
	const params = new URLSearchParams({ limit: String(limit) });

	if (filter) {
		params.set("filter", "true");
	}

	const response = await fetch(`/api/v1/histogram?${params}`, {
		headers: {
			Accept: "application/json",
		},
	});

	if (!response.ok) {
		throw new WhitelistHistogramRequestError(
			response.statusText,
			response.status,
		);
	}

	const data = (await response.json()) as ApiWhitelistHistogramBin[];

	return data.map((bin) => ({
		label: `${bin.bin_min_rank ?? 0}-${bin.bin_max_rank ?? 0}`,
		count: bin.count ?? 0,
	}));
}
