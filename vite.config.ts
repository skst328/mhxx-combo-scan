import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

// バージョンは package.json の version だけで管理し、ビルド時にアプリへ埋め込む
const { version } = JSON.parse(
  readFileSync(path.resolve(import.meta.dirname, "package.json"), "utf8"),
) as { version: string };

/**
 * 公開版の index.html に CSP を書き、外部への通信や外部のファイルの読み込みをブラウザに止めさせる
 * (動画をどこにも送らないことを、コードの書き方ではなくブラウザが守る)。
 * GitHub Pages はレスポンスヘッダーを設定できないので meta で書く。開発サーバーは inline のスクリプトと
 * HMR の WebSocket を使うので、ビルドのときだけ入れる
 */
function contentSecurityPolicy(): Plugin {
  return {
    name: "content-security-policy",
    apply: "build",
    transformIndexHtml: {
      order: "post",
      handler(html) {
        // inline のスクリプト (テーマを最初の描画前に当てるもの) は、中身のハッシュで許す
        const hashes = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)].map(
          ([, code]) => `'sha256-${createHash("sha256").update(code).digest("base64")}'`,
        );
        const policy = [
          "default-src 'none'",
          // wasm を動かすには 'wasm-unsafe-eval' が要る
          `script-src 'self' 'wasm-unsafe-eval' ${hashes.join(" ")}`,
          "style-src 'self' 'unsafe-inline'",
          // 読み込んだ動画 (長さを読むための <video>) と、切り出したコマの画像は blob: の URL で出す
          "media-src blob:",
          "img-src 'self' blob: data:",
          "font-src 'self'",
          // wasm の読み込みだけ。外への fetch・XHR・WebSocket はできない
          "connect-src 'self'",
          // Worker は blob: から作り (src/lib/worker.ts)、その中で本物のスクリプトを import する
          // (Worker の中の import も worker-src で判定される)
          "worker-src 'self' blob:",
          "manifest-src 'self'",
          "base-uri 'none'",
          "form-action 'none'",
        ].join("; ");
        const charset = '<meta charset="UTF-8" />';
        if (!html.includes(charset)) throw new Error("index.html に CSP を入れる場所 (meta charset) がない");
        // スクリプトより前に置かないと、それより前のものには効かない
        return html.replace(charset, `${charset}\n    <meta http-equiv="Content-Security-Policy" content="${policy}" />`);
      },
    },
  };
}

export default defineConfig(({ mode }) => ({
  // GitHub Pages はリポジトリ名のサブパスで配信される。開発サーバーは / のままに
  // したいので分ける。preview は本番ビルドを配信するのでサブパスになる
  base: mode === "production" ? "/mhxx-combo-scan/" : "/",
  plugins: [react(), tailwindcss(), contentSecurityPolicy()],
  define: {
    __APP_VERSION__: JSON.stringify(version),
  },
  // Worker は wasm (combo-core/pkg) を ES モジュールとして読み込む
  worker: { format: "es" },
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },
}));
