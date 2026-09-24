import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
	server: {
		proxy: {
			"/api": {
				target: "http://localhost:8080",
			},
		},
	},
	plugins: [tailwindcss(), sveltekit()],
});
