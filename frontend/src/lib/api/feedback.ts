export type FeedbackPayload = {
	id: string;
	works: boolean;
};

export class FeedbackRequestError extends Error {
	constructor(
		message: string,
		readonly status: number,
	) {
		super(message);
		this.name = "FeedbackRequestError";
	}
}

export async function submitFeedback({
	id,
	works,
}: FeedbackPayload): Promise<void> {
	const response = await fetch(
		`/api/v1/feedback/${encodeURIComponent(id)}/${works}`,
		{
			method: "POST",
		},
	);

	if (!response.ok) {
		throw new FeedbackRequestError(response.statusText, response.status);
	}
}
