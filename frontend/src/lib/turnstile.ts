type Turnstile = {
	render: (
		container: HTMLElement,
		options: {
			sitekey: string;
			action: string;
			theme: string;
			appearance: "interaction-only";
			callback: (token: string) => void;
			"expired-callback": () => void;
			"error-callback": () => void;
		},
	) => string;
	remove: (id: string) => void;
};

declare global {
	interface Window {
		turnstile?: Turnstile;
	}
}

let loading: Promise<Turnstile> | undefined;

export function loadTurnstile(): Promise<Turnstile> {
	if (window.turnstile) return Promise.resolve(window.turnstile);
	if (!loading) {
		loading = new Promise<Turnstile>((resolve, reject) => {
			const script = document.createElement("script");
			script.src =
				"https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit";
			script.async = true;
			script.onload = () => {
				if (window.turnstile) resolve(window.turnstile);
				else reject(new Error("Turnstile unavailable"));
			};
			script.onerror = () => {
				script.remove();
				reject(new Error("Turnstile unavailable"));
			};
			document.head.append(script);
		}).catch((error) => {
			loading = undefined;
			throw error;
		});
	}
	return loading;
}
