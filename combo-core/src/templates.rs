//! 二値化済みの見本。実データは build.rs が templates/*.png から生成する。

/// 二値化済みの見本 1 枚。`bits` は行優先で `w * h` 個
pub struct Template {
    pub w: usize,
    pub h: usize,
    pub bits: &'static [bool],
}

include!(concat!(env!("OUT_DIR"), "/templates_data.rs"));
