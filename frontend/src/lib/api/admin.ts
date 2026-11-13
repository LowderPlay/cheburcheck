export class AdminRequestError extends Error {
	constructor(
		public readonly status: number,
		statusText: string,
	) {
		super(
			status === 401
				? "Неверный пароль администратора"
				: `Ошибка ${status}: ${statusText}`,
		);
		this.name = "AdminRequestError";
	}
}

export async function adminRequest<T>(
	password: string,
	path: string,
	method = "GET",
	body?: unknown,
	signal?: AbortSignal,
): Promise<T> {
	const response = await fetch(`/api/v1/admin${path}`, {
		method,
		signal,
		cache: "no-store",
		headers: {
			Authorization: `Bearer ${password}`,
			Accept: "application/json",
			...(body ? { "Content-Type": "application/json" } : {}),
		},
		...(body ? { body: JSON.stringify(body) } : {}),
	});
	if (!response.ok)
		throw new AdminRequestError(response.status, response.statusText);
	return response.json() as Promise<T>;
}
