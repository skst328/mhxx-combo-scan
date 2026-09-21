import { useId, useRef } from "react";
import { FileVideo, Loader2, Upload, X } from "lucide-react";
import { cn } from "cn";
import { Button } from "@/components/ui/button";

type Props = {
  onSelect: (file: File) => void;
  /** 選んだ動画と解析結果をまとめて捨てるとき */
  onReset: () => void;
  /** 選んだ動画。未選択なら undefined */
  selected?: { name: string; detail: string };
  /** 動画を確認している最中 */
  probing?: boolean;
  disabled?: boolean;
  /** ドラッグ&ドロップの案内を出すか。タッチ環境では意味がないので隠す */
  showDropHint?: boolean;
};

/**
 * 動画を受け取る領域。`<label>` で入力を包んであるので、クリックでもタップでも
 * ファイル選択が開く。ドラッグ&ドロップはページ全体が受けるので、ここでは扱わない。
 *
 * 選んだあとはこの領域自体が選択済みの表示になる。操作した場所が変わらないと、
 * 受け付けられたことが分からない。選択済みでも受け取り続けるので、そのまま次の動画を
 * 放り込める
 */
export function VideoDropzone({
  onSelect,
  onReset,
  selected,
  probing,
  disabled,
  showDropHint = true,
}: Props) {
  const inputId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const take = (files: FileList | null) => {
    const file = files?.[0];
    if (file) onSelect(file);
    // 同じファイルを選び直せるようにする
    if (inputRef.current) inputRef.current.value = "";
  };

  const input = (
    <input
      id={inputId}
      ref={inputRef}
      type="file"
      accept="video/mp4,video/*"
      className="sr-only"
      disabled={disabled}
      onChange={(e) => take(e.target.files)}
    />
  );

  return (
    <label
      htmlFor={inputId}
      className={cn(
        "flex w-full cursor-pointer rounded-xl transition-colors",
        "bg-card hover:bg-muted/50",
        // 選択済みは実線にして、受け付けたことを分かるようにする
        selected
          ? "items-center gap-3 border px-4 py-3 text-left"
          : "flex-col items-center justify-center gap-3 border-2 border-dashed px-6 py-10 text-center",
        disabled && "pointer-events-none opacity-50",
      )}
    >
      {selected ? (
        <>
          <div className="flex size-10 shrink-0 items-center justify-center rounded-full bg-muted">
            <FileVideo className="size-5 text-primary" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium">{selected.name}</p>
            <p className="truncate text-sm text-muted-foreground">{selected.detail}</p>
          </div>
          <Button
            variant="ghost"
            size="sm"
            // label の中なので、押すとファイル選択まで開いてしまう
            onClick={(e) => {
              e.preventDefault();
              onReset();
            }}
            disabled={disabled}
          >
            <X />
            リセット
          </Button>
        </>
      ) : (
        <>
          <div className="flex size-12 items-center justify-center rounded-full bg-muted">
            {probing ? (
              <Loader2 className="size-6 animate-spin text-primary" />
            ) : (
              <Upload className="size-6 text-muted-foreground" />
            )}
          </div>
          <div className="space-y-1">
            <p className="text-sm font-medium">
              {probing
                ? "動画を確認しています…"
                : showDropHint
                  ? "動画をドラッグ&ドロップ、またはタップして選択"
                  : "タップして動画を選択"}
            </p>
            <p className="text-sm text-muted-foreground">Switch 2 で録画した 1280×720 の調合動画</p>
          </div>
        </>
      )}

      {input}
    </label>
  );
}
