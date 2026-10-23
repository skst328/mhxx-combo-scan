/**
 * Worker を作る。本物の Worker のスクリプト (url) を、blob: の小さな Worker から読み込む。
 *
 * 公開版の外部通信の禁止 (index.html の meta の CSP、vite.config.ts) は、URL から作った Worker には効かない
 * (Worker は自分のレスポンスヘッダーの CSP に従い、GitHub Pages はそれを付けない)。blob: から作った Worker は
 * ページの CSP を引き継ぐので、動画を扱う Worker の中でも外には通信できない
 */
export function createWorker(url: string): Worker {
  const source = `import ${JSON.stringify(new URL(url, window.location.href).href)}`;
  const blob = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
  const worker = new Worker(blob, { type: "module" });
  URL.revokeObjectURL(blob);
  return worker;
}
