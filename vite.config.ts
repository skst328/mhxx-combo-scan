import { readFileSync } from "node:fs";
import path from "path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// バージョンは package.json の version だけで管理し、ビルド時にアプリへ埋め込む
const { version } = JSON.parse(
  readFileSync(path.resolve(import.meta.dirname, "package.json"), "utf8"),
) as { version: string };

export default defineConfig(({ mode }) => ({
  // GitHub Pages はリポジトリ名のサブパスで配信される。開発サーバーは / のままに
  // したいので分ける。preview は本番ビルドを配信するのでサブパスになる
  base: mode === "production" ? "/mhxx-combo-scan/" : "/",
  plugins: [react(), tailwindcss()],
  define: {
    __APP_VERSION__: JSON.stringify(version),
  },
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },
}));
