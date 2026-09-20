import type { Diagnosis } from "../../combo-core/pkg/combo_core.js";
import type { SearchRequest, SearchResponse } from "@/workers/search.worker";

export type { Diagnosis, Drift } from "../../combo-core/pkg/combo_core.js";

/** 1 秒あたりのフレーム数 */
const FPS = 30;
const FRAMES_PER_DAY = 2_592_000;

/** 入力欄の文字列を非負整数にする。空や不正なら null */
export function parseFrames(text: string): number | null {
  const value = Number(text.replaceAll(",", "").trim());
  if (!Number.isFinite(value) || value < 0) return null;
  return Math.floor(value);
}

export const DEFAULT_START = 0;
export const DEFAULT_RANGE = 10 ** 8;

/** フレーム数をゲーム内の経過時間としてざっくり表す */
export function approximateDuration(frames: number): string {
  if (frames <= 0) return "0";
  const seconds = frames / FPS;
  if (seconds >= 86400) return `約${Math.round(seconds / 86400).toLocaleString()}日`;
  if (seconds >= 3600) return `約${Math.round(seconds / 3600)}時間`;
  if (seconds >= 60) return `約${Math.round(seconds / 60)}分`;
  return `約${Math.round(seconds)}秒`;
}

export type SearchProgress = { consumed: number; total: number };

export type SearchOutcome = {
  hits: number[];
  /** 見つかった総数。hits は上限で打ち切られていることがある */
  totalHits: number;
  patternLength: number;
  elapsedMs: number;
  /** 見つからなかったとき、途中で乱数がずれていれば内訳が入る */
  diagnosis?: Diagnosis;
};

/** 累計の並びが検索に使えない理由。Rust の SearchError と対応する */
type SearchError =
  | { type: "tooShort" }
  | { type: "invalidDifference"; positions: number[] }
  | { type: "unknownValue"; positions: number[] };

function isSearchError(value: unknown): value is SearchError {
  return typeof value === "object" && value !== null && "type" in value;
}

export function describeSearchError(error: unknown): string {
  if (!isSearchError(error)) {
    return error instanceof Error ? error.message : "検索に失敗しました";
  }
  switch (error.type) {
    case "tooShort":
      return "調合の回数が足りません";
    case "invalidDifference":
      return `累計の ${error.positions.join("、")} 番目が生産数として成立しません`;
    case "unknownValue":
      return `累計の ${error.positions.join("、")} 番目が確定していないので検索できません`;
    default:
      return "検索に失敗しました";
  }
}

/** フレーム位置を「日 時 分 秒 フレーム」に直す */
export function formatFrame(frame: number): string {
  const d = Math.floor(frame / FRAMES_PER_DAY);
  const h = Math.floor((frame % FRAMES_PER_DAY) / (3600 * FPS));
  const m = Math.floor((frame % (3600 * FPS)) / (60 * FPS));
  const s = Math.floor((frame % (60 * FPS)) / FPS);
  const f = frame % FPS;
  return `${d}日 ${h}時間 ${m}分 ${s}秒 ${f}フレーム`;
}

/**
 * 偽陽性が期待値 1 件を下回る範囲。生産数 1 個あたり 1.5 bit しかないので、
 * 並びが短いと広く探すほど偶然の一致が増える
 */
export function safeRange(patternLength: number): number {
  return 2 ** (1.5 * patternLength);
}

export function searchFrames(
  cumulative: (number | undefined)[],
  options: {
    start?: number;
    total: number;
    onProgress?: (progress: SearchProgress) => void;
    onStarted?: (patternLength: number) => void;
    signal?: AbortSignal;
  },
): Promise<SearchOutcome> {
  const { start = 0, total, onProgress, onStarted, signal } = options;

  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL("../workers/search.worker.ts", import.meta.url), {
      type: "module",
    });
    let patternLength = 0;

    const stop = () => {
      worker.terminate();
      signal?.removeEventListener("abort", onAbort);
    };
    function onAbort() {
      stop();
      reject(signal?.reason ?? new DOMException("中止しました", "AbortError"));
    }
    signal?.addEventListener("abort", onAbort, { once: true });

    worker.onmessage = (event: MessageEvent<SearchResponse>) => {
      const message = event.data;
      switch (message.type) {
        case "started":
          patternLength = message.patternLength;
          onStarted?.(patternLength);
          break;
        case "progress":
          onProgress?.({ consumed: message.consumed, total });
          break;
        case "done":
          stop();
          resolve({ ...message, patternLength });
          break;
        case "error":
          stop();
          reject(new Error(describeSearchError(message.error)));
          break;
      }
    };
    worker.onerror = (event) => {
      stop();
      reject(new Error(event.message || "検索を実行できませんでした"));
    };

    worker.postMessage({ cumulative, start, total } satisfies SearchRequest);
  });
}
