import { AlertCircle, AlertTriangle, Info } from "lucide-react";
import { cn } from "cn";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { formatFrame, safeRange, type SearchOutcome } from "@/lib/search";

/** 符号を必ず付ける。ずれは負にもなる */
const signed = (n: number) => (n >= 0 ? `+${n}` : String(n));

type Props = {
  outcome: SearchOutcome;
  /** 実際に探した範囲。偽陽性の判定に使う */
  range: number;
};

export function FrameResult({ outcome, range }: Props) {
  const unreliable = range > safeRange(outcome.patternLength);
  const description =
    outcome.hits.length > 0
      ? outcome.totalHits === 1
        ? "一意に特定できました"
        : `${outcome.totalHits.toLocaleString()} 件の候補が見つかりました`
      : outcome.diagnosis
        ? null
        : "この範囲では見つかりませんでした";

  return (
    <Card>
      <CardHeader>
        <CardTitle>調合開始時のフレーム位置</CardTitle>
        {/* 診断が出るときは、下の Alert が状況を説明するので重ねない */}
        {description && <CardDescription>{description}</CardDescription>}
      </CardHeader>
      <CardContent className="space-y-3">
        {outcome.hits.length === 0 ? (
          outcome.diagnosis ? (
            <div className="space-y-3">
              {outcome.diagnosis.leadingSkipped ? (
                <Alert>
                  <Info className="text-primary" />
                  <AlertTitle>先頭の 1 回を照合から外しました</AlertTitle>
                  <AlertDescription>
                    調合開始直後の 1 回が乱数と噛み合わなかったので、そこを外して照合しました。
                    残りはすべて一致しているので、補正は要りません。
                  </AlertDescription>
                </Alert>
              ) : (
                <Alert variant="warning">
                  <AlertTriangle />
                  <AlertTitle>調合の途中で乱数がずれた可能性があります</AlertTitle>
                </Alert>
              )}

              <div className="space-y-0.5 rounded-lg border p-3">
                <p className="text-sm text-muted-foreground">調合開始時のフレーム位置</p>
                <p className="font-mono text-3xl tabular-nums">
                  <span className="select-all">{outcome.diagnosis.frame}</span>
                  {outcome.diagnosis.totalDrift !== 0 && (
                    <span className="text-muted-foreground">
                      {" "}
                      ({signed(outcome.diagnosis.totalDrift)})
                    </span>
                  )}
                </p>
                <p className="text-sm text-muted-foreground">
                  ゲーム開始から {formatFrame(outcome.diagnosis.frame)} 相当
                </p>
              </div>

              {outcome.diagnosis.drifts.length > 0 && (
                <div className="space-y-1 text-sm">
                  <p>
                    {outcome.diagnosis.totalDrift === 0 ? (
                      "ずれを検出しましたが打ち消し合っているので、上の値のままで大丈夫です。"
                    ) : (
                      <>
                        括弧は検出したずれの合計です。これを足した{" "}
                        <strong className="font-mono">
                          {outcome.diagnosis.frame + outcome.diagnosis.totalDrift}
                        </strong>{" "}
                        が正しいかもしれません。
                      </>
                    )}
                  </p>
                  <ul className="list-inside list-disc text-muted-foreground">
                    {outcome.diagnosis.drifts.map((d) => (
                      <li key={d.craft}>
                        {d.craft} 回目あたりの調合で {signed(d.steps)}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              調合の途中で乱数がずれた可能性がありますが、位置は割り出せませんでした。
              撮り直すことをおすすめします
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
                {/* 区切り記号は入れない */}
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
