import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { parseSeconds } from "@/lib/video";

type Props = {
  start: string;
  end: string;
  onStartChange: (value: string) => void;
  onEndChange: (value: string) => void;
  /** 動画全体の長さ (秒)。案内に使う */
  duration: number;
  disabled?: boolean;
};

/** 動画のどこを解析するかの指定。空欄なら端まで */
export function TimeRangeFields({
  start,
  end,
  onStartChange,
  onEndChange,
  duration,
  disabled,
}: Props) {
  const from = parseSeconds(start);
  const to = parseSeconds(end);
  const ordered =
    from.ok && to.ok && (from.value === undefined || to.value === undefined || to.value > from.value);

  return (
    <div className="space-y-2">
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1.5">
          <Label className="text-sm font-normal text-muted-foreground" htmlFor="time-start">開始（秒）</Label>
          <Input
            id="time-start"
            inputMode="decimal"
            placeholder="先頭から"
            value={start}
            disabled={disabled}
            aria-invalid={!from.ok}
            onChange={(e) => onStartChange(e.target.value)}
          />
        </div>
        <div className="space-y-1.5">
          <Label className="text-sm font-normal text-muted-foreground" htmlFor="time-end">終了（秒）</Label>
          <Input
            id="time-end"
            inputMode="decimal"
            placeholder="末尾まで"
            value={end}
            disabled={disabled}
            aria-invalid={!to.ok || !ordered}
            onChange={(e) => onEndChange(e.target.value)}
          />
        </div>
      </div>
      <p className="text-sm text-muted-foreground">
        {!from.ok || !to.ok
          ? "0 以上の数値を入れてください"
          : !ordered
            ? "終了は開始より後にしてください"
            : (from.value ?? 0) === 0 && (to.value ?? Infinity) >= duration
              ? `動画全体（${duration.toFixed(1)} 秒）を解析します`
              : `${(from.value ?? 0).toFixed(1)} 秒から ${(to.value ?? duration).toFixed(1)} 秒までを解析します`}
      </p>
    </div>
  );
}
