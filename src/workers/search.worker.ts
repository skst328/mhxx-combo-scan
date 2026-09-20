import init, {
  FrameSearcher,
  diagnoseSearch,
  type Diagnosis,
} from "../../combo-core/pkg/combo_core.js";

/** 1 回の step で進める数。これを刻みに進捗を返す */
const CHUNK = 2_000_000;

/** 持ち帰る候補の上限。並びが短いと数百万件出るので、その場合は件数だけ数える */
const MAX_HITS = 200;

export type SearchRequest = {
  cumulative: (number | undefined)[];
  /** 探し始めるフレーム位置 */
  start: number;
  /** 進める総数 */
  total: number;
};

export type SearchResponse =
  | { type: "started"; patternLength: number }
  | { type: "progress"; consumed: number }
  | {
      type: "done";
      hits: number[];
      totalHits: number;
      elapsedMs: number;
      /** 見つからなかったときだけ、前半一致からの診断を試みる */
      diagnosis?: Diagnosis;
    }
  | { type: "error"; error: unknown };

const post = (message: SearchResponse) => self.postMessage(message);

self.onmessage = async (event: MessageEvent<SearchRequest>) => {
  const { cumulative, start, total } = event.data;
  let searcher: FrameSearcher | undefined;
  try {
    await init();
    // 並びが検索に使えない場合、ここで SearchError が throw される
    searcher = FrameSearcher.create(cumulative, start);
    post({ type: "started", patternLength: searcher.patternLength });

    const startedAt = performance.now();
    const hits: number[] = [];
    let totalHits = 0;
    while (searcher.consumed < total) {
      const found = searcher.step(Math.min(CHUNK, total - searcher.consumed));
      totalHits += found.length;
      // スプレッドで展開すると件数が多いときにスタックが溢れる
      for (const frame of found) {
        if (hits.length >= MAX_HITS) break;
        hits.push(frame);
      }
      post({ type: "progress", consumed: searcher.consumed });
    }
    // 1 件も無いときは、途中で乱数がずれた可能性を調べる
    const diagnosis = totalHits === 0 ? diagnoseSearch(cumulative, start, total) : undefined;
    post({
      type: "done",
      hits,
      totalHits,
      elapsedMs: performance.now() - startedAt,
      diagnosis,
    });
  } catch (error) {
    post({ type: "error", error });
  } finally {
    searcher?.free();
  }
};
