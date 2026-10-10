import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { approximateDuration, parseFrames } from "@/lib/search";

type Props = {
  start: string;
  end: string;
  onStartChange: (value: string) => void;
  onEndChange: (value: string) => void;
  disabled?: boolean;
};

/** 乱数を探す区間の指定 */
export function SearchRangeFields({
  start,
  end,
  onStartChange,
  onEndChange,
  disabled,
}: Props) {
  const startFrame = parseFrames(start);
  const endFrame = parseFrames(end);
  const ordered = startFrame !== null && endFrame !== null && endFrame > startFrame;

  return (
    <div className="space-y-2">
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1.5">
          <Label className="text-sm font-normal text-muted-foreground" htmlFor="frame-start">開始フレーム</Label>
          <Input
            id="frame-start"
            inputMode="numeric"
            value={start}
            disabled={disabled}
            aria-invalid={startFrame === null}
            onChange={(e) => onStartChange(e.target.value)}
          />
        </div>
        <div className="space-y-1.5">
          <Label className="text-sm font-normal text-muted-foreground" htmlFor="frame-end">終了フレーム</Label>
          <Input
            id="frame-end"
            inputMode="numeric"
            value={end}
            disabled={disabled}
            aria-invalid={endFrame === null || !ordered}
            onChange={(e) => onEndChange(e.target.value)}
          />
        </div>
      </div>
      <p className="text-sm text-muted-foreground">
        {startFrame === null || endFrame === null
          ? "0 以上の整数を入力してください"
          : !ordered
            ? "終了は開始より後にしてください"
            : `${startFrame.toLocaleString()} 〜 ${endFrame.toLocaleString()} を探します（${approximateDuration(endFrame - startFrame)}ぶん）`}
      </p>
    </div>
  );
}
