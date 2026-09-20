import { ALL_FORMATS, BlobSource, Input, VideoSampleSink } from "mediabunny";
import init, {
  Session,
  type Analysis,
  type FrameReading,
} from "../../combo-core/pkg/combo_core.js";

export type AnalyzeRequest = {
  file: File;
  /** 解析を始める時刻 (秒) */
  start?: number;
  /** 解析を終える時刻 (秒) */
  end?: number;
};

export type AnalyzeResponse =
  | { type: "progress"; frames: number; t: number; duration: number }
  | {
      type: "done";
      analysis: Analysis;
      frames: number;
      reachedCap: boolean;
      elapsedMs: number;
      shots: FrameShot[];
    }
  | { type: "error"; message: string };

/** 進捗の通知を間引く間隔 */
const PROGRESS_INTERVAL_MS = 100;

/** 持ち帰る画像の上限 */
const MAX_SHOTS = 200;

/** 完成品の個数が変わったコマ。結果の目視確認に使う */
export type FrameShot = { reading: FrameReading; image: Blob };

const post = (message: AnalyzeResponse) => self.postMessage(message);

async function analyze({ file, start = 0, end }: AnalyzeRequest) {
  await init();
  const startedAt = performance.now();

  const roi = Session.roi();
  const source = Session.sourceSize();

  const input = new Input({ formats: ALL_FORMATS, source: new BlobSource(file) });
  // Worker ごと終了するので現状は不要だが、使い回す形にしたときに漏れないよう明示して解放する
  let session: Session | undefined;
  try {
    const track = await input.getPrimaryVideoTrack();
    if (!track) throw new Error("映像トラックが見つかりません");

    const codedWidth = await track.getCodedWidth();
    const codedHeight = await track.getCodedHeight();
    if (codedWidth !== source.width || codedHeight !== source.height) {
      throw new Error(
        `${codedWidth}×${codedHeight} の動画です。${source.width}×${source.height} にのみ対応しています`,
      );
    }
    if (!(await track.canDecode())) {
      throw new Error("このブラウザではこの動画を再生できません");
    }

    // ROI と同じ大きさの作業領域。ここに切り出してから画素を読む
    const canvas = new OffscreenCanvas(roi.width, roi.height);
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) throw new Error("画像処理を初期化できませんでした");

    const duration = (end ?? (await input.computeDuration())) - start;
    session = new Session();
    let frames = 0;
    let reachedCap = false;
    let lastNotified = 0;
    const shots: FrameShot[] = [];
    let lastProduct: number | undefined;

    for await (const sample of new VideoSampleSink(track).samples(start, end)) {
      try {
        // 元画像の ROI をそのままの大きさで写す
        sample.draw(context, roi.x, roi.y, roi.width, roi.height, 0, 0, roi.width, roi.height);
        const { data } = context.getImageData(0, 0, roi.width, roi.height);
        // Uint8ClampedArray を同じメモリを指す Uint8Array として渡す (コピーしない)
        const bytes = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
        const reading = session.pushFrame(sample.timestamp, bytes);
        frames++;

        // 完成品の個数が変わったコマだけ画像を残す。PNG にしておけば 1 枚数十 KB で済む。
        // 素材がどちらも読めない行はクロスチェックが捨てるので、ここでも撮らない
        // (別のレシピにカーソルがある間のコマがこれにあたる)
        const usable =
          reading.crafting &&
          (reading.material1 !== undefined || reading.material2 !== undefined);
        if (usable && reading.product !== undefined && reading.product !== lastProduct) {
          if (shots.length < MAX_SHOTS) {
            shots.push({ reading, image: await canvas.convertToBlob({ type: "image/png" }) });
          }
          lastProduct = reading.product;
        }

        const now = performance.now();
        if (now - lastNotified >= PROGRESS_INTERVAL_MS) {
          lastNotified = now;
          post({ type: "progress", frames, t: sample.timestamp, duration });
        }
        if (reading.done) {
          reachedCap = true;
          break;
        }
      } finally {
        sample.close();
      }
    }

    post({
      type: "done",
      analysis: session.analyze(),
      frames,
      reachedCap,
      elapsedMs: performance.now() - startedAt,
      shots,
    });
  } finally {
    session?.free();
    input.dispose();
  }
}

self.onmessage = (event: MessageEvent<AnalyzeRequest>) => {
  analyze(event.data).catch((error: unknown) => {
    post({ type: "error", message: error instanceof Error ? error.message : "解析に失敗しました" });
  });
};
