import { useId, useRef, useState } from "react";
import { FileVideo, Upload } from "lucide-react";
import { cn } from "cn";

type Props = {
  onSelect: (file: File) => void;
  disabled?: boolean;
  /** ドラッグ&ドロップの案内を出すか。タッチ環境では意味がないので隠す */
  showDropHint?: boolean;
};

/**
 * 動画を受け取る領域。`<label>` で入力を包んであるので、クリックでもタップでも
 * ファイル選択が開く。ドラッグ&ドロップはポインタのある環境でのみ働く。
 */
export function VideoDropzone({ onSelect, disabled, showDropHint = true }: Props) {
  const inputId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);

  const take = (files: FileList | null) => {
    const file = files?.[0];
    if (file) onSelect(file);
    // 同じファイルを選び直せるようにする
    if (inputRef.current) inputRef.current.value = "";
  };

  return (
    <label
      htmlFor={inputId}
      onDragOver={(e) => {
        if (disabled) return;
        e.preventDefault();
        setDragging(true);
      }}
      onDragLeave={() => setDragging(false)}
      onDrop={(e) => {
        if (disabled) return;
        e.preventDefault();
        setDragging(false);
        take(e.dataTransfer.files);
      }}
      className={cn(
        "flex w-full cursor-pointer flex-col items-center justify-center gap-3 rounded-xl border-2 border-dashed px-6 py-10 text-center transition-colors",
        "border-border bg-card hover:bg-muted/50",
        dragging && "border-primary bg-primary/5",
        disabled && "pointer-events-none opacity-50",
      )}
    >
      <div className="flex size-12 items-center justify-center rounded-full bg-muted">
        {dragging ? (
          <FileVideo className="size-6 text-primary" />
        ) : (
          <Upload className="size-6 text-muted-foreground" />
        )}
      </div>

      <div className="space-y-1">
        <p className="text-sm font-medium">
          {showDropHint ? "動画をドラッグ&ドロップ、またはタップして選択" : "タップして動画を選択"}
        </p>
        <p className="text-sm text-muted-foreground">
          Switch 2 で録画した 1280×720 の調合動画
        </p>
      </div>

      <input
        id={inputId}
        ref={inputRef}
        type="file"
        accept="video/mp4,video/*"
        className="sr-only"
        disabled={disabled}
        onChange={(e) => take(e.target.files)}
      />
    </label>
  );
}
