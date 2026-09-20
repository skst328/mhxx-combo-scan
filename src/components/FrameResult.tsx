import { AlertCircle, AlertTriangle } from "lucide-react";
import { cn } from "cn";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { formatFrame, safeRange, type SearchOutcome } from "@/lib/search";

type Props = {
  outcome: SearchOutcome;
  /** 実際に探した範囲。偽陽性の判定に使う */
  range: number;
};

export function FrameResult({ outcome, range }: Props) {
  const unreliable = range > safeRange(outcome.patternLength);

  return (
    <Card>
      <CardHeader>
        <CardTitle>調合開始時のフレーム位置</CardTitle>
        <CardDescription>
          {outcome.hits.length === 0
            ? outcome.diagnosis
              ? "途中にずれがあるため、補正が要ります"
              : "この範囲では見つかりませんでした"
            : outcome.totalHits === 1
              ? "一意に特定できました"
              : `${outcome.totalHits.toLocaleString()} 件の候補が見つかりました`}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {outcome.hits.length === 0 ? (
          outcome.diagnosis ? (
            <div className="space-y-3">
              <Alert>
                <AlertTriangle className="text-primary" />
                <AlertTitle>途中で乱数のずれが見つかりました</AlertTitle>
                <AlertDescription>
                  並びの前半から位置は割り出せました。ただし途中にずれがあり、
                  ゲーム側の挙動なのか数値の読み違いなのかは区別できません。
                  範囲を広げても見つかりません。
                </AlertDescription>
              </Alert>

              <div className="space-y-0.5 rounded-lg border p-3">
                <p className="text-sm text-muted-foreground">調合開始時のフレーム位置</p>
                {/* mhxx-rng にそのまま貼れるよう、区切り記号を入れない */}
                <p className="font-mono text-3xl tabular-nums select-all">
                  {outcome.diagnosis.frame}
                </p>
                <p className="text-sm text-muted-foreground">
                  ゲーム開始から {formatFrame(outcome.diagnosis.frame)} 相当
                </p>
              </div>

              <div className="space-y-1 text-sm">
                <p>
                  調合終了時点の乱数は、この計算から{" "}
                  <strong className="font-mono">+{outcome.diagnosis.totalDrift}</strong>{" "}
                  フレーム分進んでいる可能性があります。
                </p>
                <ul className="list-inside list-disc text-muted-foreground">
                  {outcome.diagnosis.drifts.map((d) => (
                    <li key={d.craft}>
                      {d.craft} 回目の調合の直前で +{d.steps}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              探索範囲の外にあるか、調合の途中でゲーム側の乱数がずれた可能性があります。
              範囲を広げても見つからない場合は、撮り直してください
            </p>
          )
        ) : (
          <ul className="space-y-1.5">
            {outcome.hits.map((frame) => (
              <li
                key={frame}
                className={cn(
                  "space-y-0.5 rounded-lg border p-3",
                  outcome.totalHits === 1 && "border-primary/40 bg-primary/5",
                )}
              >
                {/* mhxx-rng にそのまま貼れるよう、区切り記号を入れない */}
                <p className="font-mono text-3xl tabular-nums select-all">{frame}</p>
                <p className="text-sm text-muted-foreground">
                  ゲーム開始から {formatFrame(frame)} 相当
                </p>
              </li>
            ))}
          </ul>
        )}

        {outcome.totalHits > outcome.hits.length && (
          <p className="text-sm text-muted-foreground">
            全 {outcome.totalHits.toLocaleString()} 件のうち {outcome.hits.length} 件を表示しています
          </p>
        )}

        {unreliable && outcome.totalHits > 0 && (
          <Alert variant="destructive">
            <AlertCircle />
            <AlertTitle>偶然の一致が混ざっている可能性があります</AlertTitle>
            <AlertDescription>
              調合 {outcome.patternLength + 1} 回ぶんの並びでは、この範囲を一意に絞れません。
              範囲を狭めるか、調合回数を増やして撮り直してください
            </AlertDescription>
          </Alert>
        )}

        <p className="text-sm text-muted-foreground">
          検索時間 {(outcome.elapsedMs / 1000).toFixed(2)} 秒
        </p>
      </CardContent>
    </Card>
  );
}
