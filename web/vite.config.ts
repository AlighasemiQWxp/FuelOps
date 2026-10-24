import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
    dedupe: [
      "react",
      "react-dom",
      "@tanstack/react-query",
      "react-router",
      "lucide-react",
      "motion",
      "recharts",
      "@radix-ui/react-dialog",
      "@radix-ui/react-slot",
      "class-variance-authority",
      "clsx",
      "tailwind-merge",
      "react-multi-date-picker",
      "react-date-object",
      "vitest",
      "@testing-library/react",
    ],
  },
  server: {
    fs: { allow: [".."] },
    proxy: {
      "/api": "http://127.0.0.1:8080",
      "/health": "http://127.0.0.1:8080",
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}", "../HandleSystem/**/*.test.tsx"],
    globals: true,
  },
});
