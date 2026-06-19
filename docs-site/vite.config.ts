import { defineConfig } from "vite";
import path from "path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: process.env.VITE_DOCS_BASE || "/",
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
});
