import { cn } from "cn";
import type { Analysis, Resolution } from "../../combo-core/pkg/combo_core.js";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

/** 割り振りの結果を「3」「2 2」「[2+4|3+3|4+2]」のように表す */
function formatYield(resolution: Resolution, count: number): string {
  switch (resolution.type) {
    case "exact":
    case "inferred":
      return resolution.yields.join(" ");
    case "capped":
      return resolution.yields ? resolution.yields.join(" ") : "?".repeat(count);
    case "ambiguous":
      return `[${resolution.candidates.map((c) => c.join("+")).join("|")}]`;
    case "failed":
      return "解析失敗";
  }
}

const NOTE: Record<Resolution["type"], string> = {
  exact: "",
  inferred: "表示を1回見落としたので補完",
  ambiguous: "割り振りが決まらない",
  capped: "上限に到達",
  failed: "説明できない",
};

/**
 * 推測が入った回。`exact` は素直に確定した回、`capped` は 99 に達しただけで
 * どちらも正常なので数えない
 */
const isUncertain = (type: Resolution["type"]) =>
  type === "inferred" || type === "ambiguous" || type === "failed";

type Props = {
  analysis: Analysis;
  frames: number;
  elapsedMs: number;
  /** 完成品が上限に達して打ち切ったか */
  reachedCap: boolean;
};

export function AnalysisResult({ analysis, frames, elapsedMs, reachedCap }: Props) {
  const total = analysis.crafts.reduce((sum, c) => sum + c.count, 0);
  // 推測が入った回。折りたたみを開く価値があるかの目安にする
  const uncertain = analysis.crafts.filter((c) => isUncertain(c.resolution.type)).length;
  const detail =
    `${frames} コマを ${(elapsedMs / 1000).toFixed(2)} 秒で読みました` +
    (reachedCap ? " ・ 完成品が上限に達したため途中で終了" : "");

  if (analysis.crafts.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>調合が見つかりませんでした</CardTitle>
          <CardDescription>{detail}</CardDescription>
        </CardHeader>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>
          調合 {total} 回 (素材 {analysis.materialFrom}→{analysis.materialTo})
        </CardTitle>
        <CardDescription>{detail}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-1">
          <p className="text-sm text-muted-foreground">累計</p>
          <p className="font-mono text-sm break-all">
            {analysis.cumulative
              .map((v) => (v === undefined ? "??" : String(v).padStart(2, "0")))
              .join(" ")}
          </p>
        </div>

        <details>
          <summary className="cursor-pointer text-sm text-muted-foreground select-none">
            調合ごとの明細
            {uncertain > 0
              ? `（${uncertain} 回は推測が入っています）`
              : "（すべて読み取った値から確定しています）"}
          </summary>
          <div className="mt-3 overflow-x-auto">
            <table className="w-full text-sm">
            <thead>
              <tr className="border-b text-sm text-muted-foreground">
                <th className="py-1.5 pr-4 text-right font-medium">時刻</th>
                <th className="py-1.5 pr-4 text-right font-medium">素材減</th>
                <th className="py-1.5 pr-4 text-right font-medium">完成品増</th>
                <th className="py-1.5 pr-4 text-right font-medium">累計</th>
                <th className="py-1.5 pr-4 text-right font-medium">生産数</th>
                <th className="py-1.5 text-left font-medium" />
              </tr>
            </thead>
            <tbody className="font-mono tabular-nums">
              {analysis.crafts.map((craft, i) => (
                <tr
                  key={i}
                  className={cn(
                    "border-b border-border/50 last:border-0",
                    isUncertain(craft.resolution.type) && "bg-muted/50",
                  )}
                >
                  <td className="py-1 pr-4 text-right whitespace-nowrap">{craft.t.toFixed(2)}s</td>
                  <td className="py-1 pr-4 text-right">-{craft.count}</td>
                  <td className="py-1 pr-4 text-right">+{craft.gain}</td>
                  <td className="py-1 pr-4 text-right">{String(craft.after).padStart(2, "0")}</td>
                  <td className="py-1 pr-4 text-right whitespace-nowrap">
                    {formatYield(craft.resolution, craft.count)}
                  </td>
                  <td
                    className={cn(
                      "py-1 font-sans text-sm whitespace-nowrap",
                      craft.resolution.type === "failed"
                        ? "text-destructive"
                        : "text-muted-foreground",
                    )}
                  >
                    {NOTE[craft.resolution.type]}
                  </td>
                </tr>
              ))}
            </tbody>
            </table>
          </div>
        </details>

        {analysis.issues.length > 0 && (
          <div className="space-y-1 text-sm text-destructive">
            {analysis.issues.map((issue, i) => (
              <p key={i}>
                {issue.type === "unexplained"
                  ? `${issue.t.toFixed(2)}s 素材${issue.count}個減・完成品+${issue.gain} は説明できません`
                  : `${issue.t.toFixed(2)}s 最後の調合の完成品の増加が読めていません`}
              </p>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
