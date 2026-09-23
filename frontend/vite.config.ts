import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
	server: {
		proxy: {
			"/api": {
				target: "https://cheburcheck.ru",
				secure: false,
				changeOrigin: true,
			},
		},
	},
	plugins: [tailwindcss(), sveltekit()],
});
