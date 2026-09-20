import { useEffect, useRef, useState } from "react";
import { AlertCircle, Loader2, Play, ShieldCheck, X } from "lucide-react";
// ハイフンがアンダースコアになることに注意
import { Session, type Size } from "../combo-core/pkg/combo_core.js";
import { AnalysisResult } from "@/components/AnalysisResult";
import { FrameGallery } from "@/components/FrameGallery";
import { FrameResult } from "@/components/FrameResult";
import { Guide } from "@/components/Guide";
import { SearchRangeFields } from "@/components/SearchRangeFields";
import { ThemeToggle } from "@/components/ThemeToggle";
import { TimeRangeFields } from "@/components/TimeRangeFields";
import { VideoDropzone } from "@/components/VideoDropzone";
import { Alert, AlertAction, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { analyzeVideo, loadCore, type AnalyzeResult, type Progress } from "@/lib/analyze";
import { detectCapabilities, hasFinePointer } from "@/lib/capabilities";
import {
  DEFAULT_RANGE,
  DEFAULT_START,
  parseFrames,
  searchFrames,
  type SearchOutcome,
  type SearchProgress,
} from "@/lib/search";
import {
  checkSource,
  formatBytes,
  formatDuration,
  parseSeconds,
  probeVideo,
  type VideoInfo,
} from "@/lib/video";

type Selection = {
  file: File;
  info: VideoInfo;
  /** 解析に進めない理由。null なら進める */
  problem: string | null;
};

/** 解析と検索は続けて走るので、どちらの途中かを持つ */
type Phase =
  | { kind: "analyze"; progress: Progress }
  | { kind: "search"; progress: SearchProgress };

type Result = {
  analyze: AnalyzeResult;
  /** 探した範囲。偽陽性の判定に使う */
  range: number;
  search: SearchOutcome | null;
  searchError: string | null;
};

function App() {
  /** wasm から受け取る、受け付けるコマの大きさ。読み込みが済んだかの目印も兼ねる */
  const [source, setSource] = useState<Size | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [fromText, setFromText] = useState("");
  const [toText, setToText] = useState("");
  const [startText, setStartText] = useState(String(DEFAULT_START));
  const [rangeText, setRangeText] = useState(String(DEFAULT_RANGE));
  const [probing, setProbing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase | null>(null);
  const [result, setResult] = useState<Result | null>(null);
  const abort = useRef<AbortController | null>(null);

  const [capabilities] = useState(detectCapabilities);
  const [finePointer] = useState(hasFinePointer);

  const running = phase !== null;
  const start = parseFrames(startText);
  const range = parseFrames(rangeText);
  const from = parseSeconds(fromText);
  const to = parseSeconds(toText);
  const timeOk =
    from.ok &&
    to.ok &&
    (from.value === undefined || to.value === undefined || to.value > from.value);
  const canRun = source !== null && start !== null && range !== null && range > 0 && timeOk;

  useEffect(() => {
    loadCore()
      .then(() => setSource(Session.sourceSize()))
      .catch(() => setError("解析エンジンを読み込めませんでした"));
  }, []);

  const reset = () => {
    setSelection(null);
    setResult(null);
    setError(null);
  };

  const select = async (file: File) => {
    if (!source) return;
    setProbing(true);
    setError(null);
    setResult(null);
    setSelection(null);
    const probed = await probeVideo(file);
    setProbing(false);
    if (!probed.ok) {
      setError(probed.reason);
      return;
    }
    // 解析範囲は動画全体を初期値にする。<video> が返す長さは実際の最終コマより
    // 手前になりうるので切り上げる。範囲が動画より長くても、末尾で止まるだけ
    setFromText("0");
    setToText(
      Number.isFinite(probed.info.duration)
        ? (Math.ceil(probed.info.duration * 10) / 10).toFixed(1)
        : "",
    );
    setSelection({ file, info: probed.info, problem: checkSource(probed.info, source) });
  };

  /** 動画の解析と乱数の検索を続けて走らせる */
  const run = async () => {
    if (!selection || start === null || range === null || !from.ok || !to.ok) return;
    const controller = new AbortController();
    abort.current = controller;
    setError(null);
    setResult(null);

    try {
      const span = (to.value ?? selection.info.duration) - (from.value ?? 0);
      setPhase({ kind: "analyze", progress: { frames: 0, t: 0, duration: span } });
      const analyzed = await analyzeVideo(selection.file, {
        start: from.value,
        end: to.value,
        onProgress: (progress) => setPhase({ kind: "analyze", progress }),
        signal: controller.signal,
      });
      // 先に明細を出しておき、検索の結果は後から差し込む
      setResult({ analyze: analyzed, range, search: null, searchError: null });
      if (analyzed.analysis.crafts.length === 0) return;

      setPhase({ kind: "search", progress: { consumed: 0, total: range } });
      try {
        const found = await searchFrames(analyzed.analysis.cumulative, {
          start,
          total: range,
          onProgress: (progress) => setPhase({ kind: "search", progress }),
          signal: controller.signal,
        });
        setResult((prev) => (prev ? { ...prev, search: found } : prev));
      } catch (e) {
        if (controller.signal.aborted) throw e;
        // 検索に失敗しても解析結果は残す
        setResult((prev) =>
          prev
            ? { ...prev, searchError: e instanceof Error ? e.message : "検索に失敗しました" }
            : prev,
        );
      }
    } catch (e) {
      if (!controller.signal.aborted) {
        setError(e instanceof Error ? e.message : "解析に失敗しました");
      }
    } finally {
      abort.current = null;
      setPhase(null);
    }
  };

  const percent =
    phase === null
      ? 0
      : phase.kind === "analyze"
        ? (phase.progress.t / (phase.progress.duration || 1)) * 100
        : (phase.progress.consumed / phase.progress.total) * 100;

  return (
    <main className="mx-auto flex min-h-dvh w-full max-w-5xl flex-col gap-6 px-4 py-8">
      <header className="flex items-start justify-between gap-3">
        <div className="space-y-1">
          <h1>MHXX 調合スキャン</h1>
          <p className="text-sm text-muted-foreground sm:text-base">
            調合の動画から、ゲーム内の乱数位置を特定します
          </p>
        </div>
        <ThemeToggle />
      </header>

      <p className="flex items-center justify-center gap-2 rounded-lg border border-primary/30 bg-primary/5 px-4 py-2.5 text-center text-sm font-medium sm:text-base">
        <ShieldCheck className="size-5 shrink-0 text-primary" />
        動画は端末の中だけで処理され、どこにも送信されません
      </p>

      {!capabilities.ready && (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertTitle>このブラウザでは動きません</AlertTitle>
          <AlertDescription>
            <ul className="list-inside list-disc">
              {capabilities.missing.map((m) => (
                <li key={m}>{m}</li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      )}

      <VideoDropzone
        onSelect={select}
        disabled={!source || !capabilities.ready || probing || running}
        showDropHint={finePointer}
      />

      <Guide open={selection === null} />

      {probing && <p className="text-sm text-muted-foreground">動画を確認しています…</p>}

      {error && (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertTitle>読み込めませんでした</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {selection && (
        <Card>
          <CardHeader>
            <CardTitle className="truncate">{selection.file.name}</CardTitle>
            <CardDescription>
              {selection.info.width}×{selection.info.height} ・{" "}
              {formatDuration(selection.info.duration)} ・ {formatBytes(selection.file.size)}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {selection.problem ? (
              <Alert variant="destructive">
                <AlertCircle />
                <AlertTitle>この動画は解析できません</AlertTitle>
                <AlertDescription>{selection.problem}</AlertDescription>
                <AlertAction>
                  <Button variant="ghost" size="xs" aria-label="選び直す" onClick={reset}>
                    <X />
                  </Button>
                </AlertAction>
              </Alert>
            ) : (
              <>
                <section className="space-y-2">
                  <h3>動画の解析範囲</h3>
                  <TimeRangeFields
                    start={fromText}
                    end={toText}
                    onStartChange={setFromText}
                    onEndChange={setToText}
                    duration={selection.info.duration}
                    disabled={running}
                  />
                </section>

                <section className="space-y-2 border-t pt-4">
                  <h3>乱数の探索範囲</h3>
                  <SearchRangeFields
                    start={startText}
                    range={rangeText}
                    onStartChange={setStartText}
                    onRangeChange={setRangeText}
                    disabled={running}
                  />
                </section>

                <div className="border-t pt-4">
                {phase ? (
                  <div className="space-y-2">
                    <div className="flex items-center justify-between gap-3">
                      <p className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Loader2 className="size-4 animate-spin" />
                        {phase.kind === "analyze"
                          ? `動画を読み取っています… ${phase.progress.frames} コマ`
                          : `乱数を探しています… ${Math.floor(percent)}%`}
                      </p>
                      <Button variant="outline" size="sm" onClick={() => abort.current?.abort()}>
                        中止
                      </Button>
                    </div>
                    <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                      <div
                        className="h-full rounded-full bg-primary transition-[width]"
                        style={{ width: `${Math.min(100, percent)}%` }}
                      />
                    </div>
                  </div>
                ) : (
                  <Button onClick={run} disabled={!canRun}>
                    <Play />
                    解析する
                  </Button>
                )}
                </div>
              </>
            )}
          </CardContent>
        </Card>
      )}

      {result?.search && <FrameResult outcome={result.search} range={result.range} />}

      {result?.searchError && (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertTitle>フレーム位置を特定できませんでした</AlertTitle>
          <AlertDescription>{result.searchError}</AlertDescription>
        </Alert>
      )}

      {result && (
        <>
          <AnalysisResult
            analysis={result.analyze.analysis}
            frames={result.analyze.frames}
            elapsedMs={result.analyze.elapsedMs}
            reachedCap={result.analyze.reachedCap}
          />
          {result.analyze.analysis.crafts.length === 0 ? (
            <FrameGallery
              shots={result.analyze.probes}
              title="切り出した画面"
              description="調合が見つからなかったので、実際に読み取った範囲を出しています"
            />
          ) : (
            <FrameGallery shots={result.analyze.shots} />
          )}
        </>
      )}
    </main>
  );
}

export default App;
