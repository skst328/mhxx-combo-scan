import { useEffect, useRef } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import type { FrameShot } from "@/lib/analyze";

/** 読めなかった箇所は「?」で示す */
const digits = (value: number | undefined) => (value === undefined ? "?" : String(value));

/**
 * Blob を img に割り当てる。URL の作成と開放を 1 箇所にまとめてあるので、
 * 付け替えや再マウントでも取りこぼさない
 */
function ShotImage({ image, alt }: { image: Blob; alt: string }) {
  const ref = useRef<HTMLImageElement>(null);

  useEffect(() => {
    const url = URL.createObjectURL(image);
    const el = ref.current;
    el?.setAttribute("src", url);
    return () => {
      el?.removeAttribute("src");
      URL.revokeObjectURL(url);
    };
  }, [image]);

  return <img ref={ref} alt={alt} className="w-full" />;
}

type Props = {
  shots: FrameShot[];
  title?: string;
  description?: string;
};

export function FrameGallery({ shots, title, description }: Props) {
  if (shots.length === 0) return null;

  const bytes = shots.reduce((sum, s) => sum + s.image.size, 0);

  return (
    <Card>
      <CardHeader>
        <CardTitle>{title ?? "読み取ったコマ"}</CardTitle>
        <CardDescription>
          {description ?? "完成品の個数が変わったコマだけを並べています"}（{shots.length} 枚 ・{" "}
          {(bytes / 1024 / 1024).toFixed(1)} MB）
        </CardDescription>
      </CardHeader>
      <CardContent>
        <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {shots.map((shot, i) => (
            <li key={i} className="space-y-1.5 overflow-hidden rounded-lg border">
              <ShotImage image={shot.image} alt={`${shot.reading.t.toFixed(2)} 秒のコマ`} />
              <p className="px-2.5 pb-2 font-mono text-sm tabular-nums">
                {shot.reading.t.toFixed(2)}s ・ 素材 {digits(shot.reading.material1)}/
                {digits(shot.reading.material2)} ・ 完成品 {digits(shot.reading.product)}
              </p>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
