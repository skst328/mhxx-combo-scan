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
      /** 調合が見つからなかったとき用。実際に切り出した画面 */
      probes: FrameShot[];
      timing: Timing;
    }
  | { type: "error"; message: string };

/** 進捗の通知を間引く間隔 */
const PROGRESS_INTERVAL_MS = 100;

/** 持ち帰る画像の上限 */
const MAX_SHOTS = 200;

/** 完成品の個数が変わったコマ。結果の目視確認に使う */
export type FrameShot = { reading: FrameReading; image: Blob };

/** 1 コマの処理の内訳 (ミリ秒の合計) */
export type Timing = {
  /** キャンバスへの描画 */
  draw: number;
  /** 画素の読み戻し */
  read: number;
  /** wasm での読み取り */
  recognize: number;
};

/** 調合が 1 つも見つからなかったときに、何を読んだのか見せるための間隔 (コマ) */
const PROBE_EVERY = 120;
/** 同上の枚数 */
const MAX_PROBES = 3;

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
    const probes: FrameShot[] = [];
    const timing: Timing = { draw: 0, read: 0, recognize: 0 };
    // 画像の書き出しに対応していない環境もある。そのときは一覧を諦めて解析だけ続ける
    let canCapture = true;

    const capture = async () => {
      try {
        return await canvas.convertToBlob({ type: "image/png" });
      } catch {
        canCapture = false; // 一度失敗したら以降は試さない
        return null;
      }
    };

    for await (const sample of new VideoSampleSink(track).samples(start, end)) {
      try {
        // 元画像を原寸のまま、ROI の左上がキャンバスの原点に来る位置に置く。
        // はみ出した部分は切り捨てられるので、結果は ROI の切り出しと同じになる。
        // 「元画像のどこを写すか」を渡す呼び方は、環境によって無視されて
        // 画面全体が縮小描画されることがあるため使わない
        const t0 = performance.now();
        sample.draw(context, -roi.x, -roi.y);

        // GPU の処理は遅延しうるので、その待ちは読み戻しの側に計上される
        const t1 = performance.now();
        const { data } = context.getImageData(0, 0, roi.width, roi.height);

        const t2 = performance.now();
        // Uint8ClampedArray を同じメモリを指す Uint8Array として渡す (コピーしない)
        const bytes = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
        const reading = session.pushFrame(sample.timestamp, bytes);

        timing.draw += t1 - t0;
        timing.read += t2 - t1;
        timing.recognize += performance.now() - t2;
        frames++;

        // 完成品の個数が変わったコマだけ画像を残す。PNG にしておけば 1 枚数十 KB で済む。
        // 素材がどちらも読めない行はクロスチェックが捨てるので、ここでも撮らない
        // (別のレシピにカーソルがある間のコマがこれにあたる)
        const usable =
          reading.crafting &&
          (reading.material1 !== undefined || reading.material2 !== undefined);
        if (usable && reading.product !== undefined && reading.product !== lastProduct) {
          if (canCapture && shots.length < MAX_SHOTS) {
            const image = await capture();
            if (image) shots.push({ reading, image });
          }
          lastProduct = reading.product;
        }

        // 何も見つからなかったときのために、間隔をあけて数枚だけ控えておく
        if (canCapture && probes.length < MAX_PROBES && (frames - 1) % PROBE_EVERY === 0) {
          const image = await capture();
          if (image) probes.push({ reading, image });
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
      probes,
      timing,
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
