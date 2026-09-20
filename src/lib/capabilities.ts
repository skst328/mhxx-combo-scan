/** ブラウザが解析に必要な機能を備えているか */
export type Capabilities = {
  /** 解析を始められるか */
  ready: boolean;
  /** 足りないものの説明。ready が true なら空 */
  missing: string[];
};

export function detectCapabilities(): Capabilities {
  const missing: string[] = [];
  // WebCodecs は https か localhost でしか使えない
  if (!window.isSecureContext) {
    missing.push("安全な接続 (https) が必要です");
  }
  if (!("VideoDecoder" in window)) {
    missing.push("動画のデコード機能 (WebCodecs) に対応していません");
  }
  if (typeof Worker === "undefined") {
    missing.push("Web Worker に対応していません");
  }
  return { ready: missing.length === 0, missing };
}

/** ポインタでの操作が主な環境か。ドラッグ&ドロップの案内を出すかの判断に使う */
export function hasFinePointer(): boolean {
  return window.matchMedia?.("(pointer: fine)").matches ?? false;
}
