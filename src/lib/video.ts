/** 動画ファイルの基本情報。解析に進める形式かを判断するのに使う */
export type VideoInfo = {
  width: number;
  height: number;
  /** 秒。取れなければ NaN */
  duration: number;
};

export type ProbeResult =
  | { ok: true; info: VideoInfo }
  | { ok: false; reason: string };

/** metadata がこの時間内に返らなければ諦める */
const PROBE_TIMEOUT_MS = 15_000;

/**
 * 動画の大きさと長さを読む。
 *
 * `<video>` の metadata を使うので、デコーダを起動せずに済む。ここで返るのは
 * 表示上の大きさで、解析で使う符号化された大きさとは異なりうる。
 */
export function probeVideo(file: File): Promise<ProbeResult> {
  return new Promise((resolve) => {
    const el = document.createElement("video");
    const url = URL.createObjectURL(file);
    let settled = false;
    const done = (result: ProbeResult) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      URL.revokeObjectURL(url);
      el.removeAttribute("src");
      resolve(result);
    };
    // onloadedmetadata も onerror も発火しない場合に備える
    const timer = setTimeout(
      () => done({ ok: false, reason: "動画を読み込めませんでした" }),
      PROBE_TIMEOUT_MS,
    );

    el.preload = "metadata";
    el.muted = true;
    el.onloadedmetadata = () => {
      if (!el.videoWidth || !el.videoHeight) {
        done({ ok: false, reason: "映像が入っていません" });
        return;
      }
      done({
        ok: true,
        info: { width: el.videoWidth, height: el.videoHeight, duration: el.duration },
      });
    };
    el.onerror = () => done({ ok: false, reason: "この形式は読み込めません" });
    el.src = url;
  });
}

/** 解析に使えるか。使えない理由は利用者向けの文言で返す */
export function checkSource(
  info: VideoInfo,
  expected: { width: number; height: number },
): string | null {
  if (info.width !== expected.width || info.height !== expected.height) {
    return (
      `${info.width}×${info.height} の動画です。` +
      `${expected.width}×${expected.height} にのみ対応しています`
    );
  }
  return null;
}

/** 秒の入力欄の解釈。空欄は「未指定」で、動画の端まで使う */
export type Seconds = { ok: true; value?: number } | { ok: false };

export function parseSeconds(text: string): Seconds {
  const trimmed = text.trim();
  if (trimmed === "") return { ok: true, value: undefined };
  const value = Number(trimmed);
  return Number.isFinite(value) && value >= 0 ? { ok: true, value } : { ok: false };
}

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds)) return "長さ不明";
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return m > 0 ? `${m}分${String(s).padStart(2, "0")}秒` : `${s}秒`;
}

export function formatBytes(bytes: number): string {
  const mb = bytes / 1024 / 1024;
  return mb >= 1 ? `${mb.toFixed(1)} MB` : `${(bytes / 1024).toFixed(0)} KB`;
}
