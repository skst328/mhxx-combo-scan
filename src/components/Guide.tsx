import { BookOpen } from "lucide-react";

/** 解析できる動画の条件。動画を選ぶと畳まれる */
export function Guide({ open }: { open: boolean }) {
  return (
    <details open={open} className="rounded-xl border bg-card px-4 py-3">
      <summary className="flex cursor-pointer items-center gap-2 text-sm font-medium select-none">
        <BookOpen className="size-4 text-muted-foreground" />
        動画の条件
      </summary>

      <div className="mt-4 space-y-1.5 text-sm">
        <ul className="list-inside list-disc space-y-1 text-muted-foreground">
          <li>Switch 2 の録画機能で撮った 1280×720・30fps</li>
          <li>
            <strong className="text-foreground">ボタンを押しっぱなしにして</strong>
            連続で調合したもの
          </li>
          <li>素材を 1 回に 1 個ずつ使い、1 回に 2〜4 個できるレシピ</li>
          <li>調合の失敗が無いこと</li>
        </ul>
        <p className="text-muted-foreground">
          調合の回数が多いほど正確です。20 回を下回ると候補が絞れないことがあります。
        </p>
      </div>
    </details>
  );
}
