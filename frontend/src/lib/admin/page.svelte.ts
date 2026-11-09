import { createQuery, useQueryClient } from "@tanstack/svelte-query";
import { onMount } from "svelte";
import { AdminRequestError, adminRequest } from "$lib/api/admin";

type AdminRequest = <T>(
	path: string,
	method?: string,
	body?: unknown,
) => Promise<T>;

const passwordKey = "cheburcheck:admin-password";
const message = (error: unknown) =>
	error instanceof Error ? error.message : String(error);

export function createAdminPage<T>(
	resource: string,
	path: string,
	onSignOut: () => void,
) {
	const client = useQueryClient();
	const queryKey = ["admin", resource] as const;
	let password = $state("");
	let enteredPassword = $state("");
	let signingIn = $state(false);
	let busy = $state("");
	let error = $state("");
	let notice = $state("");
	let unauthorized = $state(false);
	let generation = 0;

	const query = createQuery(() => ({
		queryKey,
		queryFn: ({ signal }) =>
			adminRequest<T[]>(password, path, "GET", undefined, signal),
		enabled: password.length > 0,
		staleTime: 15_000,
		refetchInterval: 30_000,
	}));
	const displayedError = $derived(
		error || (query.isError ? message(query.error) : ""),
	);
	const loginRequired = $derived(
		!password ||
			unauthorized ||
			(query.isError &&
				query.error instanceof AdminRequestError &&
				query.error.status === 401),
	);

	onMount(() => {
		password = localStorage.getItem(passwordKey) ?? "";
		enteredPassword = password;
		return () => {
			generation++;
		};
	});

	function report(cause: unknown) {
		error = message(cause);
		unauthorized = cause instanceof AdminRequestError && cause.status === 401;
	}

	async function signIn() {
		if (signingIn) return;
		const candidate = enteredPassword.trim();
		const current = ++generation;
		busy = "";
		notice = "";
		error = "";
		signingIn = true;
		try {
			const rows = await adminRequest<T[]>(candidate, path);
			if (current !== generation) return;
			await client.cancelQueries({ queryKey });
			if (current !== generation) return;
			client.setQueryData(queryKey, rows);
			password = candidate;
			unauthorized = false;
			localStorage.setItem(passwordKey, candidate);
		} catch (cause) {
			if (current === generation) report(cause);
		} finally {
			if (current === generation) signingIn = false;
		}
	}

	function signOut() {
		generation++;
		localStorage.removeItem(passwordKey);
		password = "";
		enteredPassword = "";
		signingIn = false;
		busy = "";
		error = "";
		notice = "";
		unauthorized = false;
		client.removeQueries({ queryKey: ["admin"] });
		onSignOut();
	}

	async function run<R>(
		key: string,
		operation: (request: AdminRequest) => Promise<R>,
		onSuccess: (result: R) => void | Promise<void>,
	) {
		if (busy) return;
		const current = generation;
		busy = key;
		error = "";
		notice = "";
		try {
			const auth = password;
			const result = await operation((url, method = "GET", body) =>
				adminRequest(auth, url, method, body),
			);
			if (current === generation) await onSuccess(result);
		} catch (cause) {
			if (current === generation) report(cause);
		} finally {
			if (current === generation) busy = "";
		}
	}

	return {
		query,
		get password() {
			return password;
		},
		get enteredPassword() {
			return enteredPassword;
		},
		set enteredPassword(value: string) {
			enteredPassword = value;
		},
		get signingIn() {
			return signingIn;
		},
		get busy() {
			return busy;
		},
		get error() {
			return error;
		},
		set error(value: string) {
			error = value;
		},
		get notice() {
			return notice;
		},
		set notice(value: string) {
			notice = value;
		},
		get displayedError() {
			return displayedError;
		},
		get loginRequired() {
			return loginRequired;
		},
		signIn,
		signOut,
		run,
		refresh: () => client.invalidateQueries({ queryKey }),
		cancel: () => client.cancelQueries({ queryKey }),
		updateRows(update: (rows: T[]) => T[]) {
			client.setQueryData<T[]>(queryKey, (rows) => update(rows ?? []));
		},
		async copy(value: string, success = "Скопировано") {
			await run(
				"copy",
				() => navigator.clipboard.writeText(value),
				() => {
					notice = success;
				},
			);
		},
	};
}

export type AdminPageState = Pick<
	ReturnType<typeof createAdminPage>,
	| "password"
	| "enteredPassword"
	| "signingIn"
	| "notice"
	| "displayedError"
	| "loginRequired"
	| "signIn"
	| "signOut"
>;
