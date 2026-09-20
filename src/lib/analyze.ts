import init, { type Analysis } from "../../combo-core/pkg/combo_core.js";
import type { AnalyzeRequest, AnalyzeResponse, FrameShot } from "@/workers/analyze.worker";

export type { FrameShot } from "@/workers/analyze.worker";

export type Progress = {
  /** 読み終えたコマ数 */
  frames: number;
  /** 直前のコマの時刻 (秒) */
  t: number;
  /** 解析対象の長さ (秒)。進捗率の分母に使う */
  duration: number;
};

export type AnalyzeResult = {
  analysis: Analysis;
  /** 読んだコマ数 */
  frames: number;
  /** 完成品が上限に達して打ち切ったか */
  reachedCap: boolean;
  elapsedMs: number;
  /** 完成品の個数が変わったコマの画像 */
  shots: FrameShot[];
  /** 調合が見つからなかったとき用。実際に切り出した画面 */
  probes: FrameShot[];
};

let wasmReady: Promise<void> | null = null;

/**
 * メインスレッド側の wasm 初期化。`Session.roi()` と `Session.sourceSize()` を
 * 読むために要る。解析そのものは Worker 側で別に初期化される
 */
export function loadCore(): Promise<void> {
  wasmReady ??= init().then(() => undefined);
  return wasmReady;
}

export function analyzeVideo(
  file: File,
  options: {
    start?: number;
    end?: number;
    onProgress?: (progress: Progress) => void;
    signal?: AbortSignal;
  } = {},
): Promise<AnalyzeResult> {
  const { start, end, onProgress, signal } = options;

  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL("../workers/analyze.worker.ts", import.meta.url), {
      type: "module",
    });

    const stop = () => {
      worker.terminate();
      signal?.removeEventListener("abort", onAbort);
    };
    function onAbort() {
      stop();
      reject(signal?.reason ?? new DOMException("中止しました", "AbortError"));
    }
    signal?.addEventListener("abort", onAbort, { once: true });

    worker.onmessage = (event: MessageEvent<AnalyzeResponse>) => {
      const message = event.data;
      switch (message.type) {
        case "progress":
          onProgress?.(message);
          break;
        case "done":
          stop();
          resolve(message);
          break;
        case "error":
          stop();
          reject(new Error(message.message));
          break;
      }
    };
    worker.onerror = (event) => {
      stop();
      reject(new Error(event.message || "解析を実行できませんでした"));
    };

    worker.postMessage({ file, start, end } satisfies AnalyzeRequest);
  });
}
