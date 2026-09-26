import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
	build: {
		cssTarget: ["chrome110", "firefox112", "safari15.3"],
	},
	server: {
		proxy: {
			"/api": {
				target: "http://localhost:8080",
			},
		},
	},
	plugins: [tailwindcss(), sveltekit()],
});
