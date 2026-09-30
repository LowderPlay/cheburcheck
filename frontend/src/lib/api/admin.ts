export async function adminRequest<T>(
	password: string,
	path: string,
	method = "GET",
	body?: unknown,
): Promise<T> {
	const response = await fetch("/api/v1/admin" + path, {
		method,
		cache: "no-store",
		headers: {
			Authorization: "Bearer " + password,
			Accept: "application/json",
			...(body ? { "Content-Type": "application/json" } : {}),
		},
		...(body ? { body: JSON.stringify(body) } : {}),
	});
	if (!response.ok)
		throw new Error(
			response.status === 401
				? "Неверный пароль администратора"
				: "Ошибка " + response.status + ": " + response.statusText,
		);
	return response.json() as Promise<T>;
}
