import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { approximateDuration, parseFrames } from "@/lib/search";

type Props = {
  start: string;
  range: string;
  onStartChange: (value: string) => void;
  onRangeChange: (value: string) => void;
  disabled?: boolean;
};

/** 乱数を探す区間の指定 */
export function SearchRangeFields({
  start,
  range,
  onStartChange,
  onRangeChange,
  disabled,
}: Props) {
  const startFrame = parseFrames(start);
  const rangeFrames = parseFrames(range);
  const valid = startFrame !== null && rangeFrames !== null && rangeFrames > 0;

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
          <Label className="text-sm font-normal text-muted-foreground" htmlFor="frame-range">探索範囲</Label>
          <Input
            id="frame-range"
            inputMode="numeric"
            value={range}
            disabled={disabled}
            aria-invalid={rangeFrames === null || rangeFrames <= 0}
            onChange={(e) => onRangeChange(e.target.value)}
          />
        </div>
      </div>
      <p className="text-sm text-muted-foreground">
        {valid
          ? `${startFrame.toLocaleString()} 〜 ${(startFrame + rangeFrames).toLocaleString()} を探します（${approximateDuration(rangeFrames)}ぶん）`
          : "0 以上の整数を入力してください"}
      </p>
    </div>
  );
}
