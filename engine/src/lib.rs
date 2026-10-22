//! 连锁棋共享引擎：棋盘规则 / 爆裂连锁 / AI 搜索 / 评估 / 模拟。
//!
//! 这个 crate 由 `tauri/src-tauri/src/lib.rs` 与 `docs/wasm/src/lib.rs` 里
//! 原本各自维护的一份引擎代码合并而来。两个宿主（Tauri 桌面/移动端、WASM 浏览器端）
//! 只保留自己的命令包装、设置与历史存取，引擎逻辑一律用这里的实现。
//!
//! 桌面端与 WASM 端唯一的实现差异是并行：桌面用 Rayon 多核，WASM 单线程。
//! 用 `parallel` feature 切换，业务代码里通过 `par_iter_or_seq!` 表达。

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::sync::OnceLock;

use rand::Rng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use alpha_beta_pruning::{AlphaBeta, Grade};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// 并行/串行只差这一层：桌面端多核并行，WASM 单线程顺序执行。
/// 用宏而不是函数，是因为两者的迭代器类型不同，没法统一返回类型。
#[cfg(feature = "parallel")]
macro_rules! par_iter_or_seq {
    ($v:expr) => { $v.par_iter() };
}
#[cfg(not(feature = "parallel"))]
macro_rules! par_iter_or_seq {
    ($v:expr) => { $v.iter() };
}


// ─── Board types ───

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum BorderMode {
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "wrap")]
    Wrap,
    #[serde(rename = "bounce")]
    Bounce,
    #[serde(rename = "degrade")]
    Degrade,
    /// 随机：每次爆炸随机选择一种边界行为（引擎层语义，供 AI 基准/工具直接调用；
    /// 产品层开局时由前端用时间戳种子解析为固定边界，整局不再改变，不再传入本值）
    #[serde(rename = "random")]
    Random,
}


/// 爆炸阈值模式（独立于边界模式）：3 级炸 / 4 级炸（默认）/ 5 级炸 / 随机（每步随机 3/4/5）
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum CapMode {
    #[serde(rename = "3")]
    Cap3,
    #[serde(rename = "4")]
    Cap4,
    #[serde(rename = "5")]
    Cap5,
    /// 随机：每步各自随机阈值 3/4/5（引擎层语义，供 AI 基准/工具直接调用；
    /// 产品层开局时由前端用时间戳种子解析为固定阈值，整局不再改变，不再传入本值）
    #[serde(rename = "random")]
    Random,
}

impl Default for CapMode {
    fn default() -> Self { CapMode::Cap4 }
}


#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Cell {
    pub owner: Option<usize>,
    pub count: u8,
    /// 旧版本混合模式(mixed)遗留的每格阈值字段；已停用，保留仅为兼容旧历史数据反序列化
    #[serde(default)]
    pub th: Option<u8>,
    /// 空洞：不规则棋盘里被挖掉的格子。不可落子，棋子飞进来直接消失，
    /// 评估时不计入总格数。默认 false —— 老数据反序列化后行为与从前一致。
    #[serde(default)]
    pub blocked: bool,
}

impl Cell {
    /// 空格
    pub const fn empty() -> Self { Cell { owner: None, count: 0, th: None, blocked: false } }
    /// 空洞（不规则棋盘挖掉的格子）
    pub const fn hole() -> Self { Cell { owner: None, count: 0, th: None, blocked: true } }
}


pub type GameBoard = Vec<Vec<Cell>>;


// ─── History types ───

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChainStatsPlayer {
    pub triggered: u32,
    pub max_chain: u32,
}


#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MaxChain {
    #[serde(default)]
    pub player: Option<usize>,
    #[serde(default)]
    pub length: u32,
}


#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub pieces: u32,
    pub points: u32,
}


#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TurnHistory {
    pub turn: u32,
    pub snapshot: std::collections::HashMap<String, PlayerSnapshot>,
    /// 本步落子 (x, y, player, rng_seed)，初始状态为 None
    #[serde(default)]
    pub mv: Option<[u64; 4]>,
}


#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: u64,
    pub time: String,
    pub mode: String,
    pub ai_algorithm: String,
    pub ai_depth: u32,
    #[serde(default)]
    pub game_count: u32,
    pub player_count: u32,
    pub ai_count: u32,
    pub board_size: u32,
    /// 棋盘列数（长方形棋盘专用）。缺省视为与 board_size 相同，即正方形 —— 老记录照旧。
    #[serde(default)]
    pub board_cols: Option<u32>,
    /// 不规则棋盘的空洞位图（每行一个 0/1 串）。缺省表示整盘可用 —— 回放靠它还原形状。
    #[serde(default)]
    pub board_cells: Option<Vec<String>>,
    /// 边界模式（默认/回环/反弹/降级/随机）
    #[serde(default)]
    pub border_mode: Option<String>,
    /// 爆炸阈值模式（3/4/5/random）
    #[serde(default)]
    pub cap_mode: Option<String>,
    #[serde(default)]
    pub winner: Option<usize>,
    #[serde(default)]
    pub color_names: Vec<String>,
    #[serde(default)]
    pub chain_stats: std::collections::HashMap<String, ChainStatsPlayer>,
    #[serde(default)]
    pub max_chain: MaxChain,
    #[serde(default)]
    pub history: serde_json::Value,
    /// 游戏是否已完成。false 表示未完成（可继续），true/finished: null 表示已完成
    #[serde(default)]
    pub finished: Option<bool>,
    /// 未完成游戏的完整状态 JSON（用于继续游戏），仅 finished=false 时有值
    #[serde(default)]
    pub game_state: Option<String>,
}


// ─── Process move result ───

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMoveResult {
    pub board: GameBoard,
    pub eliminated: Vec<usize>,
    /// 被淘汰玩家对应的击败者：(victim, killer)，无法确定时缺省
    #[serde(default)]
    pub killed_by: Vec<(usize, usize)>,
    pub chain_count: u32,
    pub game_over: bool,
    pub winner: Option<usize>,
    /// 连锁逐步棋盘快照：每一步爆炸后的棋盘（前端动画按此渲染，保证与引擎结果一致）
    #[serde(default)]
    pub steps: Vec<GameBoard>,
    /// 每次爆炸的格子坐标（与 steps 一一对应）
    #[serde(default)]
    pub exploded: Vec<(usize, usize)>,
}


#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SimulateResult {
    pub board: GameBoard,
    pub winner: Option<usize>,
    pub eliminated_order: Vec<usize>,
    pub killed_by: Vec<(usize, usize)>,
    pub chain_stats: std::collections::HashMap<String, ChainStatsPlayer>,
    pub max_chain: MaxChain,
    pub history: Vec<TurnHistory>,
}


/// 一键终局：场上只剩 AI 时，无动画直接模拟到分出胜负（纯逻辑，供命令与 ai_bench 基准测试复用）。
#[allow(clippy::too_many_arguments)]
pub fn simulate_to_end(
    board: GameBoard,
    size: usize,
    max_players: usize,
    cur_player: usize,
    eliminated: Vec<usize>,
    border_mode: BorderMode,
    cap_mode: CapMode,
    first_move_pos: Option<[usize; 2]>,
    game_count: u32,
    ai_configs: std::collections::HashMap<String, serde_json::Value>,
) -> SimulateResult {
    let cols = board.first().map_or(size, |r| r.len());   // 列数：长方形时与行数不同
    let mut b = board.clone();
        let mut elim = eliminated.clone();
        let mut killed_by: Vec<(usize, usize)> = Vec::new();
        let mut chain_stats: std::collections::HashMap<String, ChainStatsPlayer> = std::collections::HashMap::new();
        let mut max_chain = MaxChain { player: None, length: 0 };
        let mut history: Vec<TurnHistory> = Vec::new();
        let mut cur = cur_player;
        let mut gc = game_count;
        let mut no_move_round = 0usize;
        let max_steps = (size * cols * 4 * max_players).max(1000) as usize;
        let mut steps = 0usize;

        loop {
            steps += 1;
            if steps > max_steps {
                let alive_final: Vec<usize> = (0..max_players)
                    .filter(|p| !elim.contains(p) && has_pieces(&b, *p))
                    .collect();
                return SimulateResult {
                    board: b.clone(),
                    winner: alive_final.first().copied(),
                    eliminated_order: elim,
                    killed_by,
                    chain_stats,
                    max_chain,
                    history,
                };
            }
            let alive: Vec<usize> = (0..max_players)
                .filter(|p| !elim.contains(p) && has_pieces(&b, *p))
                .collect();
            if alive.len() <= 1 {
                return SimulateResult {
                    board: b.clone(),
                    winner: alive.first().copied(),
                    eliminated_order: elim,
                    killed_by,
                    chain_stats,
                    max_chain,
                    history,
                };
            }
            if !alive.contains(&cur) {
                let idx = alive.iter().position(|p| *p == cur).unwrap_or(0);
                cur = alive[(idx + 1) % alive.len()];
                continue;
            }
            let cfg = ai_configs
                .get(&cur.to_string())
                .cloned()
                .unwrap_or_else(|| serde_json::json!({"algorithm": "strategy"}));
            let alg = cfg.get("algorithm").and_then(|v| v.as_str()).unwrap_or("strategy").to_string();
            let depth = cfg.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
            let use_ml_eval = cfg.get("useMlEval").and_then(|v| v.as_bool()).unwrap_or(true);

            let mv = match alg.as_str() {
                "mcts" => find_best_move_mcts(&b, size, cur, depth, &elim, max_players, border_mode, cap_mode),
                "pvs" => {
                    let rnd = cfg.get("randomScale").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let mut searcher = PvsSearcher::new(cur, gc, use_ml_eval, border_mode, cap_mode, rnd);
                    searcher.find_best(&b, size, cur, max_players, &elim, depth)
                }
                "alphabeta" => {
                    let rnd = cfg.get("randomScale").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    find_best_move(&b, size, cur, depth, &elim, max_players, gc, first_move_pos, use_ml_eval, border_mode, cap_mode, rnd)
                },
                _ => find_best_move_strategy(&b, size, cur, &elim, max_players, gc, first_move_pos, border_mode, cap_mode),
            };
            let Some((x, y)) = mv else {
                no_move_round += 1;
                if no_move_round >= alive.len() {
                    return SimulateResult {
                        board: b.clone(),
                        winner: alive.first().copied(),
                        eliminated_order: elim,
                        killed_by,
                        chain_stats,
                        max_chain,
                        history,
                    };
                }
                let idx = alive.iter().position(|p| *p == cur).unwrap_or(0);
                cur = alive[(idx + 1) % alive.len()];
                continue;
            };
            no_move_round = 0;

            // 每步随机事件的确定性种子（供回放复现）：由局号与步号派生
            let step_seed = (gc as u64).wrapping_mul(1000003u64).wrapping_add(history.len() as u64);
            let (elims, chain_count, kbs) =
                process_click_with_killer(&mut b, size, x, y, cur, max_players, border_mode, cap_mode, Some(step_seed));
            killed_by.extend(kbs);
            for e in elims {
                if !elim.contains(&e) {
                    elim.push(e);
                }
            }
            if chain_count > 0 {
                let stats = chain_stats
                    .entry(cur.to_string())
                    .or_insert(ChainStatsPlayer { triggered: 0, max_chain: 0 });
                stats.triggered += 1;
                if chain_count > stats.max_chain {
                    stats.max_chain = chain_count;
                }
                if chain_count > max_chain.length {
                    max_chain = MaxChain { player: Some(cur), length: chain_count };
                }
            }
            let mut sn: std::collections::HashMap<String, PlayerSnapshot> = std::collections::HashMap::new();
            for p in 0..max_players {
                let mut pieces = 0u32;
                let mut points = 0u32;
                for row in &b {
                    for c in row {
                        if c.owner == Some(p) {
                            pieces += 1;
                            points += c.count as u32;
                        }
                    }
                }
                sn.insert(p.to_string(), PlayerSnapshot { pieces, points });
            }
            history.push(TurnHistory { turn: history.len() as u32, snapshot: sn, mv: Some([x as u64, y as u64, cur as u64, step_seed]) });

            gc += 1;
            let idx = alive.iter().position(|p| *p == cur).unwrap_or(0);
            cur = alive[(idx + 1) % alive.len()];
        }
}


// ─── 设备性能检测：AI 计算基准（单局） ───
// 前端逐局调用 bench_ai_game，每局返回精简计时结果；进度条由前端按局刷新。
// 首子布局与 bin/ai_bench.rs 保持一致（内圈均匀散布，切比雪夫距离 ≥3）。

/// 合法首子布局：内圈均匀散布，彼此切比雪夫距离 ≥3（符合「首子 12 格限制」）
pub fn spread_starts(sz: usize, n: usize) -> Vec<(usize, usize)> {
    let mut pos: Vec<(usize, usize)> = Vec::new();
    let cx = sz as f64 / 2.0 - 0.5;
    let cy = sz as f64 / 2.0 - 0.5;
    let r = (cx.min(cy) * 0.72).max(0.5);
    for i in 0..n {
        let ang = 2.0 * std::f64::consts::PI * i as f64 / n as f64 - std::f64::consts::FRAC_PI_2;
        let mut x = (cx + r * ang.cos()).round() as usize;
        let mut y = (cy + r * ang.sin()).round() as usize;
        x = x.min(sz - 1);
        y = y.min(sz - 1);
        let mut guard = 0usize;
        while pos.iter().any(|&(px, py)| {
            px.abs_diff(x).max(py.abs_diff(y)) < 3
        }) && guard < 100 {
            x = (x + 3) % sz;
            y = (y + 2) % sz;
            guard += 1;
        }
        pos.push((x, y));
    }
    pos
}


// ─── XGBoost Engine (pure Rust, no external deps) ───

/// 改进版特征维度（用 eval_board_improved 的中间值做特征）
pub const FEAT_DIM: usize = 18;


#[derive(Clone, Deserialize)]
#[allow(dead_code)]
pub struct XgbTreeModel {
    base_weights: Vec<f32>, default_left: Vec<i32>,
    left_children: Vec<i32>, right_children: Vec<i32>,
    split_conditions: Vec<f32>, split_indices: Vec<u32>,
    tree_param: XgbTreeParam,
}

#[derive(Clone, Deserialize)]
#[allow(dead_code)]
pub struct XgbTreeParam { num_nodes: String, }

#[derive(Clone, Deserialize)]
pub struct XgbGradientBoosterModel { trees: Vec<XgbTreeModel>, }

#[derive(Clone, Deserialize)]
pub struct XgbGradientBooster { model: XgbGradientBoosterModel, }

#[derive(Clone, Deserialize)]
pub struct XgbLearnerModelParam { base_score: String, }

#[derive(Clone, Deserialize)]
pub struct XgbLearner { gradient_booster: XgbGradientBooster, learner_model_param: XgbLearnerModelParam, }

#[derive(Clone, Deserialize)]
pub struct XgbModel { learner: XgbLearner, }


pub struct XgbNode { pub split_feat: usize, pub split_cond: f32, pub left: i32, pub right: i32, pub leaf: f32, pub is_leaf: bool, }

pub struct XGBoostEngine { pub trees: Vec<Vec<XgbNode>>, base_score: f32, }


impl XGBoostEngine {
    #[allow(dead_code)]
    pub fn load(path: &str) -> Result<Self, String> {
        let c = fs::read_to_string(path).map_err(|e| format!("读模型: {}", e))?;
        Self::load_from_str(&c)
    }

    pub fn load_from_str(json: &str) -> Result<Self, String> {
        let m: XgbModel = serde_json::from_str(json).map_err(|e| format!("JSON: {}", e))?;
        let base_score: f32 = m.learner.learner_model_param.base_score
            .trim_matches(|c| c == '[' || c == ']').parse().map_err(|_| "base_score".to_string())?;
        let mut trees = Vec::new();
        for t in &m.learner.gradient_booster.model.trees {
            let n = t.left_children.len();
            let mut ns = Vec::with_capacity(n);
            for i in 0..n {
                let leaf = t.left_children[i] == -1 && t.right_children[i] == -1;
                ns.push(XgbNode {
                    split_feat: if leaf { 0 } else { t.split_indices[i] as usize },
                    split_cond: t.split_conditions[i],
                    left: t.left_children[i], right: t.right_children[i],
                    leaf: if leaf { t.base_weights[i] } else { 0.0 }, is_leaf: leaf,
                });
            }
            trees.push(ns);
        }
        Ok(Self { trees, base_score })
    }

    pub fn predict(&self, feats: &[f32; FEAT_DIM]) -> (f32, f32) {
        let mut sum = 0.0f32;
        for tree in &self.trees {
            let mut i = 0i32;
            loop {
                let n = &tree[i as usize];
                if n.is_leaf { sum += n.leaf; break; }
                // 越界保护：模型特征维度与代码不匹配时用 0.0，防止 panic
                let feat_val = if (n.split_feat as usize) < feats.len() { feats[n.split_feat] } else { 0.0 };
                i = if feat_val <= n.split_cond { n.left } else { n.right };
            }
        }
        // JSON 中 base_score 是概率值，需要转为 log-odds
        let bs_logodds = (self.base_score / (1.0 - self.base_score + 1e-10)).ln();
        let raw = sum + bs_logodds;
        (raw, 1.0 / (1.0 + (-raw).exp()))
    }
}


#[allow(dead_code)]
pub fn xgb_engine() -> &'static XGBoostEngine {
    static ENG: OnceLock<XGBoostEngine> = OnceLock::new();
    ENG.get_or_init(|| {
        XGBoostEngine::load_from_str(include_str!("../xgb_model_board.json")).expect("加载 xgb_model_board.json 失败")
    })
}


/// 改进版特征提取：用 eval_board_improved 中间值做特征
/// 兼容所有棋盘大小（5-19）、玩家人数（2-10）、边界模式与爆炸阈值模式
/// 18 维：前 16 维保持与旧模型一致（c3/c2 语义不变），新增 [16]/[17] 为阈值感知的临界棋子占比
pub fn extract_features_improved(board: &GameBoard, cur: usize, max_players: usize, border_mode: BorderMode, cap_mode: CapMode) -> [f32; FEAT_DIM] {
    let sz = board.len();                             // 行数
    let cols = board.first().map_or(sz, |r| r.len());  // 列数：长方形时与行数不同
    let mut open_cells = 0i32;   // 可用格数：不规则棋盘挖掉的洞不计入
    let row_c = (sz as f32 - 1.0) * 0.5;               // 行中心
    let col_c = (cols as f32 - 1.0) * 0.5;             // 列中心

    let mut total_pieces = 0;
    let mut my_score = 0i32; let mut opp_score = 0i32;
    let mut my_territory = 0i32; let mut opp_territory = 0i32;
    let mut my_chain_threat = 0i32; let mut opp_chain_threat = 0i32;
    let mut my_pos_bonus = 0i32; let mut opp_pos_bonus = 0i32;
    let mut my_threat_prox = 0i32; let mut opp_threat_prox = 0i32;
    let mut my_pieces = 0i32; let mut opp_pieces = 0i32;
    let mut my_c2 = 0i32; let mut my_c3 = 0i32;
    let mut opp_c3 = 0i32;
    let mut my_crit = 0i32; let mut opp_crit = 0i32;
    let mut my_cdist = 0.0f32; let mut opp_cdist = 0.0f32;
    let mut alive_count = 0i32;

    for (i, row) in board.iter().enumerate() {
        let dist_center = ((i as f32 - row_c).abs() * 0.5) as i32;
        for (j, c) in row.iter().enumerate() {
            if !c.blocked { open_cells += 1; }
            if let Some(owner) = c.owner {
                total_pieces += 1;
                let d = dist_center + ((j as f32 - col_c).abs() * 0.5) as i32;
                let pos_val = 4i32.saturating_sub(d).max(0);
                let crit = crit_level(board, i, j, border_mode, cap_mode) as i32;
                if owner == cur {
                    my_score += c.count as i32;
                    my_territory += 1;
                    my_pos_bonus += pos_val;
                    my_cdist += (i as f32 - row_c).abs() + (j as f32 - col_c).abs();
                    my_pieces += 1;
                    if c.count == 2 { my_c2 += 1; }
                    if c.count >= 3 { my_c3 += 1; }
                    // 阈值感知威胁：临界等级高威胁，临界-1 中威胁
                    if (c.count as i32) >= crit { my_chain_threat += (c.count as i32) * 5; my_crit += 1; }
                    else if (c.count as i32) >= crit - 1 { my_chain_threat += 2; }
                    for &(ni, nj) in &nbrs_with_mode(board, i, j, border_mode) {
                        let nc = &board[ni][nj];
                        if nc.owner.is_some() && nc.owner != Some(cur) {
                            my_threat_prox += c.count as i32;
                        }
                    }
                } else {
                    opp_score += c.count as i32;
                    opp_territory += 1;
                    opp_pos_bonus += pos_val;
                    opp_cdist += (i as f32 - row_c).abs() + (j as f32 - col_c).abs();
                    opp_pieces += 1;
                    if c.count >= 3 { opp_c3 += 1; }
                    if (c.count as i32) >= crit { opp_chain_threat += (c.count as i32) * 5; opp_crit += 1; }
                    else if (c.count as i32) >= crit - 1 { opp_chain_threat += 2; }
                    for &(ni, nj) in &nbrs_with_mode(board, i, j, border_mode) {
                        let nc = &board[ni][nj];
                        if nc.owner == Some(cur) {
                            opp_threat_prox += c.count as i32;
                        }
                    }
                }
            }
        }
    }
    let total_cells = open_cells.max(1) as f32;   // 空洞不算地，别把特征稀释掉
    // 统计存活玩家
    for p in 0..max_players {
        if board.iter().flatten().any(|c| c.owner == Some(p)) {
            alive_count += 1;
        }
    }

    let total = total_pieces.max(1) as f32;
    let sz_f = sz as f32;

    let mut f = [0.0f32; FEAT_DIM];
    f[0] = total_pieces as f32 / total_cells;                            // density
    f[1] = alive_count as f32 / max_players.max(1) as f32;               // alive_ratio
    f[2] = my_pieces as f32 / total;                                     // my_share
    f[3] = my_c3 as f32 / my_pieces.max(1) as f32;                       // c3_share
    f[4] = my_c2 as f32 / my_pieces.max(1) as f32;                       // c2_share
    f[5] = my_territory as f32 / total_cells;                            // territory_share
    f[6] = (my_score - opp_score) as f32 / total_cells;                  // score_diff
    f[7] = (my_chain_threat - opp_chain_threat) as f32 / total_cells;    // chain_threat_diff
    f[8] = (my_pos_bonus - opp_pos_bonus) as f32 / total_cells;          // pos_bonus_diff
    f[9] = (my_threat_prox - opp_threat_prox) as f32 / total_cells;      // threat_prox_diff
    f[10] = my_score as f32 / opp_score.max(1) as f32;                   // my_score_norm
    f[11] = opp_score as f32 / my_score.max(1) as f32;                   // opp_score_norm
    f[12] = (my_cdist / my_pieces.max(1) as f32 - opp_cdist / opp_pieces.max(1) as f32) / (sz_f * 0.5);  // center_dist_balance
    f[13] = (my_territory - opp_territory) as f32 / total_cells;         // territory_balance
    f[14] = my_c3 as f32 / my_pieces.max(1) as f32;                      // my_threat_ratio (same as c3_share)
    f[15] = opp_c3 as f32 / opp_pieces.max(1) as f32;                    // opp_threat_ratio
    f[16] = my_crit as f32 / my_pieces.max(1) as f32;                    // my_crit_share（阈值感知）
    f[17] = opp_crit as f32 / opp_pieces.max(1) as f32;                  // opp_crit_share（阈值感知）
    f
}


/// 调度器：根据 per-AI 配置调用 ML 或手写评估（cap_mode 感知）
pub fn eval_board(board: &GameBoard, player: usize, game_count: u32, use_ml_eval: bool, border_mode: BorderMode, cap_mode: CapMode, user_random_scale: u32) -> i32 {
    if use_ml_eval {
        eval_board_ml(board, player, game_count, border_mode, cap_mode, user_random_scale)
    } else {
        eval_board_handcraft(board, player, game_count, border_mode, cap_mode, user_random_scale)
    }
}


// 诊断测试用
pub fn get_xgb_engine_for_test(path: &str) -> XGBoostEngine {
    XGBoostEngine::load(path).expect("加载模型失败")
}

pub fn xgb_predict(engine: &XGBoostEngine, feats: &[f32; FEAT_DIM]) -> (f32, f32) {
    engine.predict(feats)
}


/// 评估函数用的形状中心：所有可用格的重心。
/// 不能直接用 (rows-1)/2 —— 异形棋盘上 w 矩形的“几何中心”可能是个洞，
/// 位置权重会整体偏向一块根本下不了棋的区域。
fn board_centroid(board: &GameBoard) -> (f64, f64) {
    let (mut sr, mut sc, mut n) = (0f64, 0f64, 0usize);
    for (i, row) in board.iter().enumerate() {
        for (j, c) in row.iter().enumerate() {
            if !c.blocked { sr += i as f64; sc += j as f64; n += 1; }
        }
    }
    if n == 0 {
        let rows = board.len();
        let cols = board.first().map_or(rows, |r| r.len());
        return ((rows as f64 - 1.0) * 0.5, (cols as f64 - 1.0) * 0.5);
    }
    (sr / n as f64, sc / n as f64)
}

/// 增强版评估函数：在原手写评估基础上加入位置权重、邻居威胁、爆发势能
/// 深度优化——单次遍历，零额外 Vec 分配
///
/// 两条与边界模式直接相关的规则（之前都没有区分，导致回环下判断失真）：
///
/// 1. **位置权重只在非回环边界生效**。回环下每个格子四个方向都能被打到，
///    不存在“中心更好”这回事，位置分应当归零；否则 AI 会在回环棋盘上
///    无谓地往中间堆子，实测回环边界的胜率因此低了 10 个百分点。
/// 2. **贴身的价值取决于自己的点数**。临界棋子贴着对手是爆发势能（炸开能把对手
///    同化成自己的），而低点数棋子贴着对手是送子 —— 对手炸过来会把它同化并
///    +1 点，正好喂给对手一次连锁。原实现只要“邻居有对手”就加分，
///    两者混为一谈，等于鼓励 AI 把 1 点棋子贴到对手 3 点旁边。
pub fn eval_board_improved(board: &GameBoard, player: usize, game_count: u32, border_mode: BorderMode, cap_mode: CapMode, user_random_scale: u32) -> i32 {
    let sz = board.len();                             // 行数
    let cols = board.first().map_or(sz, |r| r.len());  // 列数
    let (row_c, col_c) = board_centroid(board);
    // 回环边界没有“边缘”，位置权重整体归零
    let use_position = !matches!(border_mode, BorderMode::Wrap | BorderMode::Random);
    let mut my_score = 0i32;
    let mut opp_score = 0i32;
    let mut my_territory = 0i32;
    let mut opp_territory = 0i32;
    let mut my_chain_threat = 0i32;
    let mut opp_chain_threat = 0i32;
    let mut my_pos_bonus = 0i32;     // 位置优势（靠近形状中心）
    let mut opp_pos_bonus = 0i32;
    let mut my_attack = 0i32;        // 己方临界棋子贴住对手 = 爆发势能
    let mut opp_attack = 0i32;
    let mut my_risk = 0i32;          // 己方低点棋子贴住对手 = 送子风险
    let mut opp_risk = 0i32;

    for i in 0..sz {
        let dist_center = ((i as f64 - row_c).abs() * 0.5) as i32;
        for j in 0..cols {
            let cell = &board[i][j];
            if cell.blocked { continue; }
            let d = dist_center + ((j as f64 - col_c).abs() * 0.5) as i32;
            let pos_val = if use_position {
                4i32.saturating_sub(d).max(0) // 中心~4, 形状边缘~0
            } else {
                0
            };

            match cell.owner {
                Some(p) if p == player => {
                    my_score += cell.count as i32;
                    my_territory += 1;
                    my_pos_bonus += pos_val;
                    let crit = crit_level(board, i, j, border_mode, cap_mode) as i32;
                    let cnt = cell.count as i32;
                    if cnt >= crit { my_chain_threat += cnt * 5; }
                    else if cnt >= crit - 1 { my_chain_threat += 2; }
                    // 贴身价值按点数区分：临界=势能，低点=风险
                    let touches_opp = nbrs_with_mode(board, i, j, border_mode)
                        .iter()
                        .any(|&(ni, nj)| {
                            let o = board[ni][nj].owner;
                            o.is_some() && o != Some(player)
                        });
                    if touches_opp {
                        if cnt >= crit { my_attack += cnt; }
                        else if cnt >= crit - 1 { my_attack += 1; }
                        else { my_risk += 1; }
                    }
                }
                Some(_) => {
                    opp_score += cell.count as i32;
                    opp_territory += 1;
                    opp_pos_bonus += pos_val;
                    let crit = crit_level(board, i, j, border_mode, cap_mode) as i32;
                    let cnt = cell.count as i32;
                    if cnt >= crit { opp_chain_threat += cnt * 5; }
                    else if cnt >= crit - 1 { opp_chain_threat += 2; }
                    let touches_me = nbrs_with_mode(board, i, j, border_mode)
                        .iter()
                        .any(|&(ni, nj)| board[ni][nj].owner == Some(player));
                    if touches_me {
                        if cnt >= crit { opp_attack += cnt; }
                        else if cnt >= crit - 1 { opp_attack += 1; }
                        else { opp_risk += 1; }
                    }
                }
                None => {}
            }
        }
    }

    let base = (my_score - opp_score) * 2
        + (my_territory - opp_territory)
        + (my_chain_threat - opp_chain_threat)  // chain_threat 已经 *5/*2
        + (my_pos_bonus - opp_pos_bonus) * 2    // 位置优势（回环时为 0）
        + (my_attack - opp_attack) * 3          // 临界棋子贴身：爆发势能
        - (my_risk - opp_risk) * 3;             // 低点棋子贴身：送子风险

    // 开场随机探索：前 3 局固定 40，之后线性降至用户设定值（0~30），5 局后保持设定值
    let user_scale = (user_random_scale.min(30)) as i32;
    let random_scale = if game_count < 3 {
        40
    } else if game_count < 5 {
        let t = (game_count.saturating_sub(2)) as i32; // 0..2
        40 - (40 - user_scale) * t / 2
    } else {
        user_scale
    };

    if random_scale > 0 {
        let hash: u64 = board.iter().enumerate().flat_map(|(i, row)| {
            row.iter().enumerate().map(move |(j, c)| {
                ((i as u64).wrapping_mul(31).wrapping_add(j as u64))
                    .wrapping_mul(7)
                    .wrapping_add(c.owner.unwrap_or(99) as u64)
                    .wrapping_mul(c.count as u64)
            })
        }).fold(0u64, |a, b| a.wrapping_mul(6364136223846793005).wrapping_add(b));
        let rnd = (hash % 100) as i32;
        base + rnd * random_scale / 100 - random_scale / 2
    } else {
        base
    }
}


pub fn eval_board_ml(board: &GameBoard, player: usize, game_count: u32, border_mode: BorderMode, cap_mode: CapMode, user_random_scale: u32) -> i32 {
    let engine = xgb_engine();
    let feats = extract_features_improved(board, player, max_players_for(board), border_mode, cap_mode);
    let (raw_score, _prob) = engine.predict(&feats);
    // XGBoost log-odds (~[-30,30]) 缩放到搜索敏感量级
    let ml_score = (raw_score * 12.0) as i32;
    // 与手写评估混合（2/3 ML + 1/3 手写），兼顾 ML 的深度学习与手写的稳定边界
    let hand_score = eval_board_improved(board, player, game_count, border_mode, cap_mode, user_random_scale);
    (ml_score * 2 + hand_score) / 3
}


/// 获取当前棋盘的玩家数
pub fn max_players_for(board: &GameBoard) -> usize {
    let mut players: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for row in board {
        for c in row {
            if let Some(owner) = c.owner {
                players.insert(owner);
            }
        }
    }
    players.len().max(2)
}


// ─── Neighbors ───

/// 邻居迭代器，只返回有效邻居（防止角落/边缘格子返回误填充的 (0,0)）
#[inline]
pub fn nbrs(i: usize, j: usize, rows: usize, cols: usize) -> Vec<(usize, usize)> {
    let mut r = Vec::with_capacity(4);
    if i > 0 { r.push((i - 1, j)); }
    if i + 1 < rows { r.push((i + 1, j)); }
    if j > 0 { r.push((i, j - 1)); }
    if j + 1 < cols { r.push((i, j + 1)); }
    r
}



/// 沿一个方向走一步，越界时回环到另一侧。
#[inline]
fn wrap_step(rows: usize, cols: usize, x: usize, y: usize, dx: isize, dy: isize) -> (usize, usize) {
    let nx = (x as isize + dx).rem_euclid(rows as isize) as usize;
    let ny = (y as isize + dy).rem_euclid(cols as isize) as usize;
    (nx, ny)
}


/// 回环边界的落点：沿方向前进，绕过棋盘边界、跳过空洞，
/// 返回第一个可用格（不含起点）。整圈都被挖空时返回 None。
/// 异形棋盘的洞因此不会"吃掉"本该绕到对面的棋子。
pub fn wrap_target(board: &GameBoard, x: usize, y: usize, dx: isize, dy: isize) -> Option<(usize, usize)> {
    let rows = board.len();
    let cols = board.first().map_or(0, |r| r.len());
    if rows == 0 || cols == 0 { return None; }
    let (mut cx, mut cy) = (x, y);
    for _ in 0..rows.max(cols) {
        let (nx, ny) = wrap_step(rows, cols, cx, cy, dx, dy);
        if (nx, ny) == (x, y) { break; }   // 绕回起点：没有别的可用格
        if !board[nx][ny].blocked { return Some((nx, ny)); }
        cx = nx; cy = ny;
    }
    None
}


/// 直线方向上第一个可用格（不回环）；界外或整段空洞都走完仍没有时就返回 None。
pub fn first_open_in_dir(board: &GameBoard, x: usize, y: usize, dx: isize, dy: isize) -> Option<(usize, usize)> {
    let rows = board.len();
    let cols = board.first().map_or(0, |r| r.len());
    let (mut cx, mut cy) = (x as isize + dx, y as isize + dy);
    while cx >= 0 && cy >= 0 && (cx as usize) < rows && (cy as usize) < cols {
        let (ux, uy) = (cx as usize, cy as usize);
        if !board[ux][uy].blocked { return Some((ux, uy)); }
        cx += dx; cy += dy;
    }
    None
}


/// 根据边界模式取邻居。回环模式会绕到对侧并跳过空洞；
/// 其余模式只返回界内的可用格 —— 洞不是邻居。
pub fn nbrs_with_mode(board: &GameBoard, i: usize, j: usize, border_mode: BorderMode) -> Vec<(usize, usize)> {
    let rows = board.len();
    let cols = board.first().map_or(0, |r| r.len());
    match border_mode {
        BorderMode::Wrap => {
            let mut r = Vec::with_capacity(4);
            for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                if let Some(p) = wrap_target(board, i, j, dx, dy) { r.push(p); }
            }
            r
        }
        _ => {
            let mut r = Vec::with_capacity(4);
            if i > 0 && !board[i - 1][j].blocked { r.push((i - 1, j)); }
            if i + 1 < rows && !board[i + 1][j].blocked { r.push((i + 1, j)); }
            if j > 0 && !board[i][j - 1].blocked { r.push((i, j - 1)); }
            if j + 1 < cols && !board[i][j + 1].blocked { r.push((i, j + 1)); }
            r
        }
    }
}



#[inline]
pub fn has_pieces(board: &GameBoard, player: usize) -> bool {
    board.iter().flatten().any(|c| c.owner == Some(player))
}


pub fn is_in_first_move_restricted(x: usize, y: usize, fx: usize, fy: usize) -> bool {
    let dx = if x >= fx { x - fx } else { fx - x };
    let dy = if y >= fy { y - fy } else { fy - y };
    // (2,0) / (-2,0)
    if dx == 2 && dy == 0 { return true; }
    // (1,-1), (1,0), (1,1) / (-1,-1)... 
    if dx == 1 && dy <= 1 { return true; }
    // (0,-2), (0,-1), (0,1), (0,2) (but not (0,0))
    if dx == 0 && dy >= 1 && dy <= 2 { return true; }
    false
}


pub fn is_in_any_restricted_zone(board: &GameBoard, x: usize, y: usize) -> bool {
    // 尺寸直接由棋盘推导，不再让调用方传一遍（长方棋盘的列数也一样）
    for i in 0..board.len() {
        for j in 0..board[i].len() {
            if board[i][j].owner.is_some() && is_in_first_move_restricted(x, y, i, j) {
                return true;
            }
        }
    }
    false
}


/// 计算格子的爆炸阈值：cap3/cap5 固定；随机每步随机 3/4/5；默认(4)保留降级边界位置修正
/// 形状外沿计数：该可用格朝四个方向里有几个是"界外或空洞"。
/// 异形棋盘的洞被视作形状边界，因此矩形与不规则棋盘共用同一套边界语义。
pub fn exposure_count(board: &GameBoard, x: usize, y: usize) -> u32 {
    let rows = board.len();
    let cols = board.first().map_or(0, |r| r.len());
    let mut n = 0u32;
    if x == 0 || board[x - 1][y].blocked { n += 1; }
    if x + 1 >= rows || board[x + 1][y].blocked { n += 1; }
    if y == 0 || board[x][y - 1].blocked { n += 1; }
    if y + 1 >= cols || board[x][y + 1].blocked { n += 1; }
    n
}


/// 格子的爆炸阈值（无 RNG 版本，供 AI 走法生成/评估/排序使用）。
/// cap3/cap5 固定；cap4 默认 4；degrade 按形状外沿数：0→4、1→3、≥2→2。
/// 矩形棋盘上等价于原来的「角 2 / 边 3 / 内部 4」，异形棋盘则沿形状轮廓降级。
#[inline]
pub fn cell_threshold(board: &GameBoard, x: usize, y: usize, border_mode: BorderMode, cap_mode: CapMode) -> u32 {
    match cap_mode {
        CapMode::Cap3 => 3,
        CapMode::Cap5 => 5,
        CapMode::Random => 4,
        CapMode::Cap4 => match border_mode {
            BorderMode::Degrade => match exposure_count(board, x, y) {
                0 => 4,
                1 => 3,
                _ => 2,
            },
            _ => 4,
        },
    }
}


/// 实际爆炸时使用的阈值：random 模式每步随机 3/4/5，其余与 cell_threshold 一致。
pub fn explosion_threshold(board: &GameBoard, x: usize, y: usize, border_mode: BorderMode, cap_mode: CapMode, rng: &mut Box<dyn rand::RngCore>) -> u32 {
    match cap_mode {
        CapMode::Random => 3 + rng.gen_range(0..3),
        _ => cell_threshold(board, x, y, border_mode, cap_mode),
    }
}


/// 己方棋子的“临界等级”：达到该等级后再落一子即爆炸（阈值 - 1）。
/// cap3→2、cap4→3、cap5→4；degrade 沿形状轮廓修正。
#[inline]
pub fn crit_level(board: &GameBoard, x: usize, y: usize, border_mode: BorderMode, cap_mode: CapMode) -> u32 {
    cell_threshold(board, x, y, border_mode, cap_mode) - 1
}


pub fn process_click_with_killer_inner(
    board: &mut GameBoard,
    sz: usize,
    x: usize,
    y: usize,
    player: usize,
    _max_players: usize,
    border_mode: BorderMode,
    cap_mode: CapMode,
    seed: Option<u64>,
    collect: bool,
) -> (Vec<usize>, u32, Vec<(usize, usize)>, Vec<GameBoard>, Vec<(usize, usize)>) {
    let cols = board.first().map_or(sz, |r| r.len());   // 列数：长方形时与行数不同
    // collect owners before
    let before: HashSet<usize> = board
        .iter()
        .flatten()
        .filter_map(|c| c.owner)
        .collect();

    // 连锁逐步快照：前端动画按引擎真实结果渲染，杜绝 cap3/cap5 随机特殊格方向跳变
    let mut steps: Vec<GameBoard> = Vec::new();
    let mut exploded: Vec<(usize, usize)> = Vec::new();

    // check first move (no pieces yet) BEFORE mutable borrow
    let is_first = board[x][y].owner.is_none() && !has_pieces(board, player);

    // 随机事件（速爆/重炮/随机模式）驱动：seed 有值则确定性可复现（回放/AI 搜索），否则真随机
    let mut rng: Box<dyn rand::RngCore> = match seed {
        Some(se) => Box::new(rand::rngs::StdRng::seed_from_u64(se)),
        None => Box::new(rand::rngs::StdRng::from_entropy()),
    };

    // place / upgrade
    if board[x][y].blocked { return (vec![], 0, vec![], steps, exploded); }   // 空洞不可落子
    if board[x][y].owner.is_none() {
        board[x][y].owner = Some(player);
        // 首子等级 = 阈值 n-1（临界态：再落一子即炸）；其余落子 +1
        let th = explosion_threshold(board, x, y, border_mode, cap_mode, &mut rng);
        board[x][y].count = if is_first { th.saturating_sub(1) as u8 } else { 1 };
    } else if board[x][y].owner == Some(player) {
        board[x][y].count += 1;
    } else {
        return (vec![], 0, vec![], steps, exploded);
    }

    // chain reaction (FIFO = VecDeque)
    let mut chain: VecDeque<(usize, usize)> = VecDeque::new();
    // victim -> killer：记录每个玩家最后一次被谁覆盖棋子（最后覆盖者即击败者）
    let mut last_killer: HashMap<usize, usize> = HashMap::new();
    // 首子放置不进连锁（首子等级 = 阈值 n-1 为临界态，本步仅"放置"；
    // 避免 random 模式下首子阈值与连锁阈值不一致导致首子立即爆炸）
    if !is_first {
        chain.push_back((x, y));
    }
    let mut chain_count: u32 = 0;
    // 连锁防御上限：正常对局连锁远低于此值；
    // 防特定棋盘结构（相邻格子互相供能）下无限互炸导致死锁/卡死
    const MAX_CHAIN_STEPS: u32 = 100_000;
    let mut chain_steps: u32 = 0;

    while let Some((cx, cy)) = chain.pop_front() {
        chain_steps += 1;
        if chain_steps > MAX_CHAIN_STEPS { break; }
        // 随机边界模式：每次爆炸随机选择一种边界行为；其余模式固定
        let eff_bm = match border_mode {
            BorderMode::Random => match rng.gen_range(0..4) {
                0 => BorderMode::Default,
                1 => BorderMode::Wrap,
                2 => BorderMode::Bounce,
                _ => BorderMode::Degrade,
            },
            _ => border_mode,
        };
        // 爆炸阈值（混合读每格 th；cap3/cap5 固定；随机每步随机 3/4/5；默认保留降级位置修正）
        let threshold = explosion_threshold(board, cx, cy, eff_bm, cap_mode, &mut rng);

        if board[cx][cy].count as u32 >= threshold {
            let owner = board[cx][cy].owner.unwrap();
            board[cx][cy].count = 0;
            board[cx][cy].owner = None;
            chain_count += 1;

            // ── 按有效边界模式生成扩散目标；cap3/cap5 的随机特殊格对所有边界模式生效 ──
            // 异形棋盘：洞与界外一样算形状边界。回环绕过洞落到对侧第一个可用格，
            // 反弹把撞到洞的能量弹到正对方向的第一个可用格；默认边界仍然让棋子消失在洞里。
            let (targets, bounce_extra): (Vec<(usize, usize)>, Vec<(usize, usize)>) = match eff_bm {
                BorderMode::Wrap => {
                    let mut t = Vec::with_capacity(4);
                    for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                        if let Some(p) = wrap_target(board, cx, cy, dx, dy) { t.push(p); }
                    }
                    (t, vec![])
                }
                BorderMode::Bounce => {
                    // 反弹边界：撞到界外或空洞的方向，能量弹到正对方向的第一个可用格
                    // 方向排列：[上, 下, 左, 右]，方向对：0↔1 (上下)，2↔3 (左右)
                    let dirs: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
                    let opposite = [1usize, 0, 3, 2];
                    let mut targets = Vec::with_capacity(4);
                    let mut extra = Vec::with_capacity(4);
                    for (i, &(dx, dy)) in dirs.iter().enumerate() {
                        let nx = cx as isize + dx;
                        let ny = cy as isize + dy;
                        let hit_wall = nx < 0 || ny < 0
                            || nx as usize >= sz || ny as usize >= cols
                            || board[nx as usize][ny as usize].blocked;
                        if hit_wall {
                            let (ox, oy) = dirs[opposite[i]];
                            if let Some(p) = first_open_in_dir(board, cx, cy, ox, oy) { extra.push(p); }
                        } else {
                            targets.push((nx as usize, ny as usize));
                        }
                    }
                    if targets.is_empty() { (targets, vec![]) } else { (targets, extra) }
                }
                _ => (nbrs(cx, cy, sz, cols), vec![]),  // 默认/降级：标准邻居扩散
            };

            // 速爆(cap3)：随机一个扩散格加 0（该方向完全不变，不产生棋子）
            // 重炮(cap5)：随机一个扩散格加 2（空格变 2 级；有棋子则在原等级上加 2）
            let special = match cap_mode {
                CapMode::Cap3 | CapMode::Cap5 => {
                    if targets.is_empty() {
                        None
                    } else {
                        Some(rng.gen_range(0..targets.len()))
                    }
                }
                _ => None,
            };

            let mut apply = |board: &mut GameBoard, nx: usize, ny: usize, ti: usize, owner: usize, last_killer: &mut HashMap<usize, usize>| {
                if let Some(sp) = special {
                    if ti == sp {
                        if cap_mode == CapMode::Cap3 {
                            // 速爆(cap3)：随机一个方向"加 0"——该格完全不变（空格保持空，
                            // 有棋子保持原样），不产生棋子、不入连锁
                            return;
                        } else {
                            // 重炮(cap5)：随机一个方向"加 2"——空格变 2 级，有棋子原等级 +2
                            if let Some(pv) = board[nx][ny].owner {
                                if pv != owner { last_killer.insert(pv, owner); }
                            }
                            board[nx][ny].owner = Some(owner);
                            board[nx][ny].count = board[nx][ny].count.saturating_add(2);
                            chain.push_back((nx, ny));
                            return;
                        }
                    }
                }
                if board[nx][ny].blocked { return; }   // 飞进空洞：棋子直接消失（等价默认边界的飞出棋盘）
                if let Some(pv) = board[nx][ny].owner {
                    if pv != owner { last_killer.insert(pv, owner); }
                }
                board[nx][ny].owner = Some(owner);
                board[nx][ny].count = board[nx][ny].count.saturating_add(1);
                chain.push_back((nx, ny));
            };

            for (ti, &(nx, ny)) in targets.iter().enumerate() {
                apply(&mut *board, nx, ny, ti, owner, &mut last_killer);
            }
            // 反弹能量（跳过特殊格：被清空/置 2 的格不再接收反弹能量）
            for &(nx, ny) in &bounce_extra {
                let is_special = special.is_some_and(|sp| {
                    targets.get(sp).map_or(false, |&(sx, sy)| sx == nx && sy == ny)
                });
                if !is_special && !board[nx][ny].blocked {
                    board[nx][ny].count = board[nx][ny].count.saturating_add(1);
                }
            }

            // 记录本次爆炸后的棋盘快照（供前端动画渲染，保证与引擎最终结果一致）
            if collect {
                steps.push(board.clone());
                exploded.push((cx, cy));
            }
        }
    }

    // owners after
    let after: HashSet<usize> = board
        .iter()
        .flatten()
        .filter_map(|c| c.owner)
        .collect();

    let mut eliminated: Vec<usize> = before.difference(&after).copied().collect();
    eliminated.sort(); // 确定顺序，避免前端排行错乱
    // 为每个被淘汰玩家确定击败者（最后一次覆盖其棋子的玩家）
    let mut killed_by: Vec<(usize, usize)> = Vec::new();
    for &victim in &eliminated {
        if let Some(&killer) = last_killer.get(&victim) {
            killed_by.push((victim, killer));
        }
    }
    (eliminated, chain_count, killed_by, steps, exploded)
}


/// 原公开版本：不收集快照（AI 搜索 / 基准测试等内部调用，性能关键路径）
pub fn process_click_with_killer(
    board: &mut GameBoard,
    sz: usize,
    x: usize,
    y: usize,
    player: usize,
    _max_players: usize,
    border_mode: BorderMode,
    cap_mode: CapMode,
    seed: Option<u64>,
) -> (Vec<usize>, u32, Vec<(usize, usize)>) {
    let (eliminated, chain_count, killed_by, _, _) =
        process_click_with_killer_inner(board, sz, x, y, player, _max_players, border_mode, cap_mode, seed, false);
    (eliminated, chain_count, killed_by)
}


/// 带连锁逐步快照的版本：steps 为每一步爆炸后的棋盘，exploded 为对应爆炸格坐标
/// （前端动画按引擎真实结果渲染，杜绝 JS 模拟与引擎随机不一致导致的方向跳变）
pub fn process_click_with_snapshots(
    board: &mut GameBoard,
    sz: usize,
    x: usize,
    y: usize,
    player: usize,
    max_players: usize,
    border_mode: BorderMode,
    cap_mode: CapMode,
    seed: Option<u64>,
) -> (Vec<usize>, u32, Vec<(usize, usize)>, Vec<GameBoard>, Vec<(usize, usize)>) {
    process_click_with_killer_inner(board, sz, x, y, player, max_players, border_mode, cap_mode, seed, true)
}


/// 无击败者信息的简单版本（供 AI 搜索/基准测试等内部调用）
pub fn process_click(
    board: &mut GameBoard,
    sz: usize,
    x: usize,
    y: usize,
    player: usize,
    max_players: usize,
    border_mode: BorderMode,
    cap_mode: CapMode,
    seed: Option<u64>,
) -> (Vec<usize>, u32) {
    let (eliminated, chain_count, _) = process_click_with_killer(board, sz, x, y, player, max_players, border_mode, cap_mode, seed);
    (eliminated, chain_count)
}


pub fn eval_board_handcraft(board: &GameBoard, player: usize, game_count: u32, border_mode: BorderMode, cap_mode: CapMode, user_random_scale: u32) -> i32 {
    let mut my_score = 0i32;
    let mut opp_score = 0i32;
    let mut my_territory = 0i32;
    let mut opp_territory = 0i32;
    let mut my_chain_threat = 0i32;
    let mut opp_chain_threat = 0i32;

    // 临界等级按格算：降级边界下形状边缘的格子阈值更低，
    // 原来按 cap_mode 硬编码成 3，异形/降级棋盘上会漏判真实的临界棋子。
    for (i, row) in board.iter().enumerate() {
        for (j, cell) in row.iter().enumerate() {
            if cell.blocked { continue; }
            let crit = crit_level(board, i, j, border_mode, cap_mode) as i32;
            let cnt = cell.count as i32;
            match cell.owner {
                Some(p) if p == player => {
                    my_score += cnt;
                    my_territory += 1;
                    if cnt >= crit { my_chain_threat += cnt * 4; }
                    else if cnt >= crit - 1 { my_chain_threat += 1; }
                }
                Some(_) => {
                    opp_score += cnt;
                    opp_territory += 1;
                    if cnt >= crit { opp_chain_threat += cnt * 4; }
                    else if cnt >= crit - 1 { opp_chain_threat += 1; }
                }
                None => {}
            }
        }
    }

    let base = (my_score - opp_score) * 2 
        + (my_territory - opp_territory)
        + (my_chain_threat - opp_chain_threat) * 3;

    // 开场随机探索：前 3 局固定 40，之后线性降至用户设定值（0~30），5 局后保持设定值
    let user_scale = (user_random_scale.min(30)) as f64;
    let random_scale = if game_count < 3 {
        40.0
    } else if game_count < 5 {
        let t = (game_count.saturating_sub(2)) as f64; // 0..2
        40.0 - (40.0 - user_scale) * t / 2.0
    } else {
        user_scale
    };

    if random_scale > 0.0 {
        // 使用确定性伪随机（基于棋盘哈希），避免多线程问题
        let hash: u64 = board.iter().enumerate().flat_map(|(i, row)| {
            row.iter().enumerate().map(move |(j, c)| {
                ((i as u64).wrapping_mul(31).wrapping_add(j as u64))
                    .wrapping_mul(7)
                    .wrapping_add(c.owner.unwrap_or(99) as u64)
                    .wrapping_mul(c.count as u64)
            })
        }).fold(0u64, |a, b| a.wrapping_mul(6364136223846793005).wrapping_add(b));
        let rnd = (hash % 100) as f64 / 100.0; // 0..1
        base + (rnd * random_scale - random_scale * 0.5) as i32
    } else {
        base
    }
}


// ─── Move generation & ordering ───

pub fn get_moves(board: &GameBoard, sz: usize, player: usize, _first_move_pos: Option<(usize, usize)>, border_mode: BorderMode, cap_mode: CapMode) -> Vec<(usize, usize)> {
    let cols = board.first().map_or(sz, |r| r.len());
    let has_p = has_pieces(board, player);
    let mut moves = Vec::new();
    for i in 0..sz {
        for j in 0..cols {
            let c = &board[i][j];
            if c.blocked { continue; }   // 空洞不可落子
            if has_p {
                // 阈值感知：只有 count < 本格阈值 的棋子可以落子（cap5 下 count==4 的引爆动作合法）
                if c.owner == Some(player) && (c.count as u32) < cell_threshold(board, i, j, border_mode, cap_mode) {
                    moves.push((i, j));
                }
            } else {
                if c.owner.is_none() && !is_in_any_restricted_zone(board, i, j) {
                    moves.push((i, j));
                }
            }
        }
    }
    moves
}


pub fn order_moves(moves: Vec<(usize, usize)>, board: &GameBoard, player: usize, border_mode: BorderMode, cap_mode: CapMode) -> Vec<(usize, usize)> {
    let mut scored: Vec<(i32, (usize, usize))> = moves
        .into_iter()
        .map(|(i, j)| {
            let c = &board[i][j];
            let mut score = c.count as i32 * 10;
            // 阈值感知：临界等级（再落一子即炸）的走法优先
            if (c.count as u32) >= crit_level(board, i, j, border_mode, cap_mode) {
                score += 100;
            }
            // 回环模式：邻居按边界模式计算（棋盘最上格的上方是最后一行）
            let near_opp: i32 = nbrs_with_mode(board, i, j, border_mode)
                .iter()
                .filter_map(|&(ni, nj)| {
                    let nc = &board[ni][nj];
                    if nc.owner.is_some() && nc.owner != Some(player) {
                        Some(nc.count as i32)
                    } else {
                        None
                    }
                })
                .sum();
            score += near_opp * 5;
            (score, (i, j))
        })
        .collect();
    scored.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, m)| m).collect()
}


// ─── GameState: alpha_beta_pruning::AlphaBeta trait impl ───

/// 游戏状态包装器，实现 AlphaBeta trait 进行并行 Alpha-Beta 搜索
#[derive(Clone)]
pub struct GameState {
    board: GameBoard,
    sz: usize,
    player: usize,
    ai_player: usize,
    eliminated: Vec<usize>,
    max_players: usize,
    game_count: u32,
    use_ml_eval: bool,
    border_mode: BorderMode,
    cap_mode: CapMode,
    user_random_scale: u32,
}


impl AlphaBeta<(usize, usize)> for GameState {
    fn evaluate(&self) -> Grade {
        // 终局判断：自己被淘汰→Min，自己获胜→Max
        if self.eliminated.contains(&self.ai_player) {
            return Grade::Min;
        }
        let alive: Vec<usize> = (0..self.max_players)
            .filter(|p| !self.eliminated.contains(p) && has_pieces(&self.board, *p))
            .collect();
        if alive.len() == 1 && alive[0] == self.ai_player {
            return Grade::Max;
        }
        // 非终局：按点数和棋子数打分（cap_mode 感知）
        Grade::Score(eval_board(&self.board, self.ai_player, self.game_count, self.use_ml_eval, self.border_mode, self.cap_mode, self.user_random_scale) as i64)
    }
    fn get_moves(&self) -> Vec<(usize, usize)> {
        get_moves(&self.board, self.sz, self.player, None, self.border_mode, self.cap_mode)
    }

    fn set(&mut self, m: &(usize, usize)) {
        let (x, y) = *m;
        let (elim, _chain) = process_click(&mut self.board, self.sz, x, y, self.player, self.max_players, self.border_mode, self.cap_mode, Some(0));
        for &e in &elim {
            if !self.eliminated.contains(&e) {
                self.eliminated.push(e);
            }
        }
        // 切换到下一个活跃玩家
        let alive: Vec<usize> = (0..self.max_players)
            .filter(|p| !self.eliminated.contains(p) && (*p == self.player || has_pieces(&self.board, *p)))
            .collect();
        if alive.len() > 1 {
            let idx = alive.iter().position(|&p| p == self.player).unwrap_or(0);
            self.player = alive[(idx + 1) % alive.len()];
        }
    }

    fn unset(&mut self, _m: &(usize, usize)) {
        // 约束：run() 和 alpha_beta() 重写使用克隆替代 set/unset，
        // 不依赖 trait 的 set/unset 配对机制。
        // 如果未来 trait 更新或重写方法被移除，请改为真正实现 set/unset 配对。
        panic!("unset() not supported — use clone-based approach (run/alpha_beta overrides)")
    }

    /// 重写 run：加入时间限制、走法排序、首步居中偏好
    fn run(&self, depth: usize) -> Option<(usize, usize)> {
        let all_moves = self.get_moves();
        if all_moves.is_empty() { return None; }
        if all_moves.len() == 1 { return Some(all_moves[0]); }

        // 首步：居中偏好
        if !has_pieces(&self.board, self.player) {
            // 行列从棋盘推导：长方形与不规则棋盘都不能拿 sz 当两轴用
            // （曾经用 self.sz 当行列，长方形上会 index out of bounds）
            let rows = self.board.len();
            let cols = self.board.first().map_or(rows, |r| r.len());
            let mut candidates: Vec<(usize, usize)> = (1..rows.saturating_sub(1))
                .flat_map(|i| (1..cols.saturating_sub(1)).filter_map(move |j| {
                    if !self.board[i][j].blocked && self.board[i][j].owner.is_none() && !is_in_any_restricted_zone(&self.board, i, j) {
                        Some((i, j))
                    } else { None }
                })).collect();
            if candidates.is_empty() {
                for i in 0..rows { for j in 0..cols {
                    if !self.board[i][j].blocked && self.board[i][j].owner.is_none() && !is_in_any_restricted_zone(&self.board, i, j) {
                        candidates.push((i, j));
                    }
                }}
            }
            if candidates.is_empty() {
                for i in 0..rows { for j in 0..cols {
                    if !self.board[i][j].blocked && self.board[i][j].owner.is_none() { candidates.push((i, j)); }
                }}
            }
            if candidates.is_empty() { return None; }
            let row_c = rows as f64 / 2.0 - 0.5;
            let col_c = cols as f64 / 2.0 - 0.5;
            return candidates.into_iter()
                .min_by(|&(i1, j1), &(i2, j2)| {
                    let d1 = (i1 as f64 - row_c).abs() + (j1 as f64 - col_c).abs();
                    let d2 = (i2 as f64 - row_c).abs() + (j2 as f64 - col_c).abs();
                    d1.partial_cmp(&d2).unwrap()
                });
        }

        // 走法排序 + 限制前10（阈值感知 + 回环邻居）
        let ordered = order_moves(all_moves, &self.board, self.player, self.border_mode, self.cap_mode);
        let max_eval = ordered.len().min(10);
        if max_eval == 0 { return None; }

        // Rayon 并行根搜索（利用 crate 的 Grade + AlphaBeta trait）
        par_iter_or_seq!(ordered[..max_eval])
            .map(|m| {
                let mut child = self.clone();
                child.set(m);
                let grade = if depth > 0 {
                    child.alpha_beta(Grade::Min, Grade::Max, depth - 1, false)
                } else {
                    child.evaluate()
                };
                (grade, *m)
            })
            .max_by(|(g1, _), (g2, _)| g1.cmp(g2))
            .map(|(_, m)| m)
    }

    /// 重写 alpha_beta：使用克隆替代 set/unset，支持多玩家轮换 + 走法排序提升剪枝
    fn alpha_beta(&mut self, mut alpha: Grade, mut beta: Grade, depth: usize, _is_max: bool) -> Grade {
        let mut moves = self.get_moves();
        if depth == 0 || moves.is_empty() {
            return self.evaluate();
        }
        // 阈值感知排序：临界走法优先 → 更多剪枝
        moves = order_moves(moves, &self.board, self.player, self.border_mode, self.cap_mode);
        // 根据实际轮到谁确定 max/min
        let is_max = self.player == self.ai_player;
        if is_max {
            let mut best = Grade::Min;
            for m in &moves {
                let mut child = self.clone();
                child.set(m);
                let grade = child.alpha_beta(alpha, beta, depth - 1, false);
                best = best.max(grade);
                alpha = alpha.max(grade);
                if beta <= alpha { break; }
            }
            best
        } else {
            let mut best = Grade::Max;
            for m in &moves {
                let mut child = self.clone();
                child.set(m);
                let grade = child.alpha_beta(alpha, beta, depth - 1, true);
                best = best.min(grade);
                beta = beta.min(grade);
                if alpha >= beta { break; }
            }
            best
        }
    }
}


// ─── PVS (NegaMax) + Killer/History + QSearch ───

pub const PVS_MAX_DEPTH: usize = 64;

pub const PVS_HISTORY_SIZE: usize = 40;


/// 紧凑 eliminated bitset（最多支持 32 位玩家）
#[derive(Clone, Copy)]
pub struct ElimSet(u32);


impl ElimSet {
    pub fn from_slice(s: &[usize]) -> Self {
        let mut e = Self(0);
        for &p in s { e.0 |= 1u32 << (p as u32); }
        e
    }
    pub fn add(&mut self, p: usize) { self.0 |= 1u32 << (p as u32); }
    pub fn contains(&self, p: usize) -> bool { (self.0 >> (p as u32)) & 1 != 0 }
}


pub struct PvsSearcher {
    game_count: u32,
    killers: [[Option<(usize, usize)>; PVS_MAX_DEPTH]; 2],
    history: [[i32; PVS_HISTORY_SIZE]; PVS_HISTORY_SIZE],
    ai_player: usize,
    use_ml_eval: bool,
    border_mode: BorderMode,
    cap_mode: CapMode,
    user_random_scale: u32,
}


impl PvsSearcher {
    pub fn new(ai_player: usize, game_count: u32, use_ml_eval: bool, border_mode: BorderMode, cap_mode: CapMode, user_random_scale: u32) -> Self {
        Self {
            game_count,
            killers: [[None; PVS_MAX_DEPTH]; 2],
            history: [[0; PVS_HISTORY_SIZE]; PVS_HISTORY_SIZE],
            ai_player,
            use_ml_eval,
            border_mode,
            cap_mode,
            user_random_scale,
        }
    }

    /// 单次遍历生成 + 评分 + 排序（替代 get_moves + order_moves_pvs 两次遍历）
    pub fn get_moves_ordered(&self, board: &GameBoard, sz: usize, player: usize, depth: usize) -> Vec<(usize, usize)> {
        let cols = board.first().map_or(sz, |r| r.len());
        let has_p = has_pieces(board, player);
        let k0 = self.killers[0][depth];
        let k1 = self.killers[1][depth];
        // 预分配
        let cap = sz * cols;
        let mut moves: Vec<(i64, (usize, usize))> = Vec::with_capacity(cap);

        if has_p {
            for i in 0..sz { for j in 0..cols {
                let c = &board[i][j];
                // 阈值感知：cap5 下 count==4 的引爆走法合法且关键
                if c.owner == Some(player) && (c.count as u32) < cell_threshold(board, i, j, self.border_mode, self.cap_mode) {
                    let mut score = c.count as i64 * 10;
                    if (c.count as u32) >= crit_level(board, i, j, self.border_mode, self.cap_mode) { score += 100; }
                    // 邻居对手分数（回环模式用 nbrs_with_mode）
                    self.add_neighbor_scores(&mut score, board, i, j, player);
                    // Killer bonus
                    if Some((i, j)) == k0 { score += 1_000_000; }
                    else if Some((i, j)) == k1 { score += 500_000; }
                    // History bonus
                    if i < PVS_HISTORY_SIZE && j < PVS_HISTORY_SIZE {
                        score += self.history[i][j] as i64 * 100;
                    }
                    moves.push((score, (i, j)));
                }
            }}
        } else {
            for i in 0..sz { for j in 0..cols {
                if board[i][j].owner.is_none() && !is_in_any_restricted_zone(board, i, j) {
                    let mut score = 0i64;
                    // 邻居对手分数（回环模式用 nbrs_with_mode）
                    self.add_neighbor_scores(&mut score, board, i, j, player);
                    moves.push((score, (i, j)));
                }
            }}
        }
        moves.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        moves.into_iter().map(|(_, m)| m).collect()
    }

    /// 邻居对手分数：回环模式按边界模式取邻居，其余模式手动展开避免 Vec 分配
    pub fn add_neighbor_scores(&self, score: &mut i64, board: &GameBoard, i: usize, j: usize, player: usize) {
        // 回环：绕行后的第一个可用格才算邻居（洞被跳过）；其余模式洞直接排除。
        for &(ni, nj) in &nbrs_with_mode(board, i, j, self.border_mode) {
            let nc = &board[ni][nj];
            if nc.owner.is_some() && nc.owner != Some(player) { *score += nc.count as i64 * 5; }
        }
    }

    /// QSearch: 仅搜索爆炸性走法，使用 bitset 加速 eliminated 判断
    pub fn quiescence(
        &mut self,
        board: &GameBoard,
        sz: usize,
        player: usize,
        max_players: usize,
        elim: ElimSet,
        alpha: i32,
        beta: i32,
        depth: i32,
    ) -> i32 {
        let cols = board.first().map_or(sz, |r| r.len());
        // 终局判断
        if elim.contains(self.ai_player) {
            return if player == self.ai_player { i32::MIN + 1000 } else { i32::MAX - 1000 };
        }
        let alive_cnt = (0..max_players)
            .filter(|p| !elim.contains(*p) && has_pieces(board, *p))
            .count();
        if alive_cnt <= 1 {
            // AI获胜→返回MIN→调用者取反→MAX→AI看好棋
            return i32::MIN + 1000;
        }
        // QSearch 最多 3 层（防长链爆炸耗死）
        if depth >= 3 { return eval_board(board, player, self.game_count, self.use_ml_eval, self.border_mode, self.cap_mode, self.user_random_scale); }
        let stand_pat = eval_board(board, player, self.game_count, self.use_ml_eval, self.border_mode, self.cap_mode, self.user_random_scale);
        if stand_pat >= beta { return beta; }
        let mut alpha = if stand_pat > alpha { stand_pat } else { alpha };

        // 收集 count >= 3 的己方棋子（爆炸性走法）
        // 用局部数组避免 Vec 分配
        let mut moves_buf = [(0usize, 0usize); 100];
        let mut n = 0usize;
        for i in 0..sz { for j in 0..cols {
            let c = &board[i][j];
            let min_explosive = match self.cap_mode {
                CapMode::Cap3 => 1,
                CapMode::Cap5 => 3,
                CapMode::Random => 2,
                CapMode::Cap4 => match self.border_mode {
                    // degrade：本格的临界等级已经按形状外沿算好（越靠边越低）
                    BorderMode::Degrade => crit_level(board, i, j, self.border_mode, self.cap_mode) as u8,
                    _ => 3,
                },
            };
            if c.owner == Some(player) && c.count >= min_explosive && (c.count as u32) < cell_threshold(board, i, j, self.border_mode, self.cap_mode) {
                if n < moves_buf.len() { moves_buf[n] = (i, j); n += 1; }
            }
        }}

        for &m in &moves_buf[..n] {
            let mut child = board.clone();
            let (new_elim, _) = process_click(&mut child, sz, m.0, m.1, player, max_players, self.border_mode, self.cap_mode, Some(0));
            let mut child_elim = elim;
            for &e in &new_elim { child_elim.add(e); }
            let next = next_live_player_es(&child, sz, player, child_elim, max_players);
            let score = -self.quiescence(&child, sz, next, max_players, child_elim, -beta, -alpha, depth + 1);
            if score >= beta { return beta; }
            if score > alpha { alpha = score; }
        }
        alpha
    }

    pub fn pvs(
        &mut self,
        board: &GameBoard,
        sz: usize,
        player: usize,
        max_players: usize,
        elim: ElimSet,
        depth: usize,
        mut alpha: i32,
        beta: i32,
        use_qsearch: bool,
    ) -> i32 {
        // 终局判断：AI被淘汰→大负，AI获胜→大正
        if elim.contains(self.ai_player) {
            return if player == self.ai_player { i32::MIN + 1000 } else { i32::MAX - 1000 };
        }
        let alive_cnt = (0..max_players)
            .filter(|p| !elim.contains(*p) && has_pieces(board, *p))
            .count();
        if alive_cnt <= 1 {
            // AI获胜→返回MIN→调用者取反→MAX→AI看好棋
            return i32::MIN + 1000;
        }
        if depth == 0 {
            if use_qsearch {
                return self.quiescence(board, sz, player, max_players, elim, alpha, beta, 0);
            }
            return eval_board(board, player, self.game_count, self.use_ml_eval, self.border_mode, self.cap_mode, self.user_random_scale);
        }

        let moves = self.get_moves_ordered(board, sz, player, depth);
        // 深层少分支，浅层多分支
        let max_branch = moves.len().min(8usize + (3usize).saturating_sub(depth) * 3);
        if max_branch == 0 { return eval_board(board, player, self.game_count, self.use_ml_eval, self.border_mode, self.cap_mode, self.user_random_scale); }

        let mut best_score = i32::MIN + 1;

        for (idx, &m) in moves[..max_branch].iter().enumerate() {
            let mut child = board.clone();
            let (new_elim, _) = process_click(&mut child, sz, m.0, m.1, player, max_players, self.border_mode, self.cap_mode, Some(0));
            let mut child_elim = elim;
            for &e in &new_elim { child_elim.add(e); }
            let next = next_live_player_es(&child, sz, player, child_elim, max_players);

            let score = if idx == 0 {
                -self.pvs(&child, sz, next, max_players, child_elim, depth - 1, -beta, -alpha, true)
            } else {
                let s = -self.pvs(&child, sz, next, max_players, child_elim, depth - 1, -alpha - 1, -alpha, false);
                if s > alpha && s < beta {
                    -self.pvs(&child, sz, next, max_players, child_elim, depth - 1, -beta, -alpha, true)
                } else { s }
            };

            if score > best_score { best_score = score; }
            if score > alpha {
                alpha = score;
                self.killers[1][depth] = self.killers[0][depth];
                self.killers[0][depth] = Some(m);
            }
            if m.0 < PVS_HISTORY_SIZE && m.1 < PVS_HISTORY_SIZE {
                self.history[m.0][m.1] += (depth * depth) as i32;
            }
            if alpha >= beta { break; }
        }
        best_score
    }

    pub fn find_best(
        &mut self,
        board: &GameBoard,
        sz: usize,
        player: usize,
        max_players: usize,
        eliminated: &[usize],
        depth: usize,
    ) -> Option<(usize, usize)> {
        let ordered = self.get_moves_ordered(board, sz, player, depth);
        if ordered.is_empty() { return None; }
        if ordered.len() == 1 { return Some(ordered[0]); }
        if !has_pieces(board, player) {
            return first_move_center(board, sz);
        }

        let elim_root = ElimSet::from_slice(eliminated);
        // 根层走法限制
        let root_limit = ordered.len().min(10usize.saturating_add(depth.saturating_sub(1) * 2));
        if root_limit == 0 { return ordered.into_iter().next(); }

        // ─── 根级并行搜索（每个分支独立 killer/history） ───
        let ai_player = self.ai_player;
        let gc = self.game_count;
        let ml = self.use_ml_eval;
        let bm = self.border_mode;
        let results: Vec<_> = par_iter_or_seq!(ordered[..root_limit])
            .map(|&m| {
                let mut child = board.clone();
                let (new_elim, _) = process_click(&mut child, sz, m.0, m.1, player, max_players, self.border_mode, self.cap_mode, Some(0));
                let mut child_elim = elim_root;
                for &e in &new_elim { child_elim.add(e); }
                let next = next_live_player_es(&child, sz, player, child_elim, max_players);

                let mut searcher = PvsSearcher::new(ai_player, gc, ml, bm, self.cap_mode, self.user_random_scale);
                let score = if depth > 0 {
                    -searcher.pvs(&child, sz, next, max_players, child_elim, depth - 1, i32::MIN + 1, i32::MAX - 1, false)
                } else {
                    eval_board(&child, player, gc, ml, bm, self.cap_mode, self.user_random_scale)
                };
                (score, m)
            })
            .collect();

        results.into_iter().max_by(|(a, _), (b, _)| a.cmp(b)).map(|(_, m)| m)
    }
}


/// 基于 ElimSet 的 next_live_player（栈数组替代 Vec 分配）
pub fn next_live_player_es(board: &GameBoard, _sz: usize, current: usize, elim: ElimSet, max_players: usize) -> usize {
    let mut alive = [0usize; 10];
    let mut count = 0;
    for p in 0..max_players {
        if !elim.contains(p) && (p == current || has_pieces(board, p)) {
            alive[count] = p;
            count += 1;
        }
    }
    if count <= 1 { return current; }
    let idx = alive[..count].iter().position(|&p| p == current).unwrap_or(0);
    alive[(idx + 1) % count]
}


// ─── MCTS (Monte Carlo Tree Search) ───

/// 轻量级 XorShift PRNG（无外部依赖）
pub struct XorShift(u64);


impl XorShift {
    pub fn seed(seed: u64) -> Self { Self(if seed == 0 { 1 } else { seed }) }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn next_usize(&mut self, range: usize) -> usize {
        if range <= 1 { return 0; }
        (self.next_u64() as usize) % range
    }
}


/// MCTS 常量
pub const UCB_C: f64 = 2.0;

pub const MCTS_ITER_PER_DEPTH: usize = 400; // iterations = depth * 400（原 800 耗时过长，减半仍保留深度语义）
pub const MCTS_PLAYOUT_MAX: usize = 24; // 模拟步数上限（原 40：链式大棋盘下是主要耗时，减到 24 兼顾速度与评估质量）
pub const MCTS_TREE_MAX_NODES: usize = 2000; // 每棵树最大节点数（防内存暴涨）

/// ─── MCTS 树节点（扁平向量存储） ───

/// 子边：一个走法 + 展开后的子节点索引
#[derive(Clone)]
pub struct MctsEdge {
    x: usize,
    y: usize,
    child: Option<usize>,
}


/// MCTS 搜索树
pub struct MctsTree {
    visits: Vec<u32>,
    wins: Vec<f64>,
    players: Vec<usize>,
    boards: Vec<GameBoard>,
    eliminateds: Vec<Vec<usize>>,
    children: Vec<Vec<MctsEdge>>,
    sz: usize,
    max_players: usize,
    ai_player: usize,
    max_nodes: usize,
    border_mode: BorderMode,
    cap_mode: CapMode,
}


impl MctsTree {
    pub fn new(board: GameBoard, eliminated: Vec<usize>, player: usize, sz: usize, max_players: usize, ai_player: usize, max_nodes: usize, border_mode: BorderMode, cap_mode: CapMode) -> Self {
        Self {
            visits: vec![0],
            wins: vec![0.0],
            players: vec![player],
            boards: vec![board],
            eliminateds: vec![eliminated],
            children: vec![Vec::new()],
            sz,
            max_players,
            ai_player,
            max_nodes,
            border_mode,
            cap_mode,
        }
    }

    /// 添加一个新节点，返回 node_idx
    pub fn add_node(&mut self, board: GameBoard, eliminated: Vec<usize>, player: usize) -> Option<usize> {
        if self.visits.len() >= self.max_nodes { return None; }
        let idx = self.visits.len();
        self.visits.push(0);
        self.wins.push(0.0);
        self.players.push(player);
        self.boards.push(board);
        self.eliminateds.push(eliminated);
        self.children.push(Vec::new());
        Some(idx)
    }

    /// 获取节点的当前合法走法集（排除已展开的子节点）
    /// 阈值感知：cap5 下 count==4 的引爆走法也包含在内
    pub fn untried_moves(&self, node: usize) -> Vec<(usize, usize)> {
        let all = get_moves(&self.boards[node], self.sz, self.players[node], None, self.border_mode, self.cap_mode);
        let kids = &self.children[node];
        // children 通常很小，线性扫描比 HashSet 更快（避免哈希分配）
        all.into_iter().filter(|m| !kids.iter().any(|e| e.x == m.0 && e.y == m.1)).collect()
    }

    /// 节点是否终局
    pub fn is_terminal(&self, node: usize) -> bool {
        let alive: Vec<usize> = (0..self.max_players)
            .filter(|p| !self.eliminateds[node].contains(p) && has_pieces(&self.boards[node], *p))
            .collect();
        alive.len() <= 1
    }

    /// UCB1 公式
    pub fn ucb(&self, parent: usize, child: usize) -> f64 {
        let pv = self.visits[parent] as f64;
        let cv = self.visits[child] as f64;
        if cv == 0.0 { return f64::MAX; }
        self.wins[child] / cv + UCB_C * (pv.ln() / cv).sqrt()
    }

    /// 用 UCB1 选择最佳已展开子节点
    pub fn select_child(&self, node: usize) -> Option<(usize, usize, usize)> {
        let kids: Vec<_> = self.children[node].iter()
            .filter_map(|e| e.child.map(|c| (e.x, e.y, c)))
            .collect();
        if kids.is_empty() { return None; }
        kids.into_iter()
            .max_by(|&(_, _, a), &(_, _, b)| self.ucb(node, a).partial_cmp(&self.ucb(node, b)).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// 展开一个节点：从已算好的 untried 中选一个（依 order_moves 排序），创建子节点
    /// 由 iterate 传入 untried，避免 select 阶段重复计算合法走法
    pub fn expand_with(&mut self, node: usize, mut untried: Vec<(usize, usize)>) -> Option<usize> {
        if self.visits.len() >= self.max_nodes { return None; }
        if untried.is_empty() { return None; }

        // 用 order_moves 排序后，优先展开最有希望的走法（阈值感知 + 回环邻居）
        untried = order_moves(untried, &self.boards[node], self.players[node], self.border_mode, self.cap_mode);
        let (mx, my) = untried[0];

        // 应用走法得到新状态
        let mut new_board = self.boards[node].clone();
        let mut new_elim = self.eliminateds[node].clone();
        let (new_elim_players, _) = process_click(&mut new_board, self.sz, mx, my, self.players[node], self.max_players, self.border_mode, self.cap_mode, Some(0));
        for &e in &new_elim_players {
            if !new_elim.contains(&e) { new_elim.push(e); }
        }
        let next_p = next_live_player(&new_board, self.sz, self.players[node], &new_elim, self.max_players);

        if let Some(child_idx) = self.add_node(new_board, new_elim, next_p) {
            // 更新父节点的 children 边
            self.children[node].push(MctsEdge { x: mx, y: my, child: Some(child_idx) });
            Some(child_idx)
        } else {
            None
        }
    }

    /// 一次完整的 MCTS 迭代：SELECT → (可能 EXPAND) → PLAYOUT → BACKPROPAGATE
    pub fn iterate(&mut self, rng: &mut XorShift) {
        // 1) SELECT: 从根出发沿 UCB1 下探直到叶节点
        let mut path = vec![0usize];
        let mut leaf = 0;

        loop {
            if self.is_terminal(leaf) { break; }
            let untried = self.untried_moves(leaf);
            if !untried.is_empty() && self.visits[leaf] >= 3 {
                // 访问足够次数后才展开（渐进展开，避免过早分裂）
                if let Some(new_node) = self.expand_with(leaf, untried) {
                    path.push(new_node);
                    leaf = new_node;
                }
                break;
            }
            // 全部展开 → UCB1 选择
            if let Some((_, _, child)) = self.select_child(leaf) {
                path.push(child);
                leaf = child;
            } else {
                break;
            }
        }

        // 2) PLAYOUT
        let score = mcts_playout(
            &self.boards[leaf], self.sz, self.players[leaf],
            &self.eliminateds[leaf], self.max_players, self.ai_player, rng,
            self.border_mode, self.cap_mode,
        );

        // 3) BACKPROPAGATE
        for &idx in &path {
            self.visits[idx] += 1;
            self.wins[idx] += score;
        }
    }

}


/// 随机模拟一局，返回 AI 玩家视角的得分 [0.0, 1.0]
pub fn mcts_playout(
    board: &GameBoard,
    sz: usize,
    start_player: usize,
    eliminated: &[usize],
    max_players: usize,
    ai_player: usize,
    rng: &mut XorShift,
    border_mode: BorderMode,
    cap_mode: CapMode,
) -> f64 {
    let mut b = board.clone();
    let mut elim = eliminated.to_vec();
    let mut cur = start_player;

    for _ in 0..MCTS_PLAYOUT_MAX {
        let alive: Vec<usize> = (0..max_players)
            .filter(|p| !elim.contains(p) && has_pieces(&b, *p))
            .collect();
        if alive.len() <= 1 {
            return if alive.first().copied() == Some(ai_player) { 1.0 } else { 0.0 };
        }

        let moves = get_moves(&b, sz, cur, None, border_mode, cap_mode);
        if moves.is_empty() {
            cur = next_live_player(&b, sz, cur, &elim, max_players);
            continue;
        }

        let idx = rng.next_usize(moves.len());
        let (x, y) = moves[idx];
        let (new_elim, _) = process_click(&mut b, sz, x, y, cur, max_players, border_mode, cap_mode, Some(0));
        for &e in &new_elim {
            if !elim.contains(&e) { elim.push(e); }
        }

        cur = next_live_player(&b, sz, cur, &elim, max_players);
    }

    // 达到最大深度：启发式评估
    let alive: Vec<usize> = (0..max_players)
        .filter(|p| !elim.contains(p) && has_pieces(&b, *p))
        .collect();
    if alive.len() == 1 {
        return if alive[0] == ai_player { 1.0 } else { 0.0 };
    }

    let (my_pts, opp_pts): (i32, i32) = b.iter().flatten().fold((0, 0), |(my, opp), c| {
        match c.owner {
            Some(p) if p == ai_player => (my + c.count as i32 + 1, opp),
            Some(_) => (my, opp + c.count as i32 + 1),
            None => (my, opp),
        }
    });
    let total = (my_pts + opp_pts).max(1);
    0.5 + (my_pts - opp_pts) as f64 / total as f64 * 0.5
}


/// MCTS 主搜索（根级并行 + 完整树搜索）
pub fn mcts_search(
    board: &GameBoard,
    sz: usize,
    ai_player: usize,
    iterations: usize,
    eliminated: &[usize],
    max_players: usize,
    border_mode: BorderMode,
    cap_mode: CapMode,
    first_move_pos: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    let all_moves = get_moves(board, sz, ai_player, first_move_pos, border_mode, cap_mode);
    if all_moves.is_empty() { return None; }
    if all_moves.len() == 1 { return Some(all_moves[0]); }

    // 首步居中偏好
    if !has_pieces(board, ai_player) {
        return first_move_center(board, sz);
    }

    // 走法排序，分枝限制
    let ordered = order_moves(all_moves, board, ai_player, border_mode, cap_mode);
    let branches = ordered.len().min(15);
    let iters_per = (iterations / branches).max(50);
    let max_nodes = MCTS_TREE_MAX_NODES / branches.max(1);

    // ─── Root-parallel MCTS ───
    // 每个根级候选走法获得独立的搜索树，在子树内执行完整 MCTS
    let results: Vec<_> = par_iter_or_seq!(ordered[..branches]).map(|&(mx, my)| {
        // 应用根走法得到子树根状态
        let mut b = board.clone();
        let mut elim = eliminated.to_vec();
        let (new_elim, _) = process_click(&mut b, sz, mx, my, ai_player, max_players, border_mode, cap_mode, Some(0));
        for &e in &new_elim {
            if !elim.contains(&e) { elim.push(e); }
        }
        let next_p = next_live_player(&b, sz, ai_player, &elim, max_players);

        // 构建子树
        let mut tree = MctsTree::new(b, elim, next_p, sz, max_players, ai_player, max_nodes, border_mode, cap_mode);
        let mut rng = XorShift::seed(rand::thread_rng().gen::<u64>());

        for _ in 0..iters_per {
            tree.iterate(&mut rng);
        }

        // 返回根节点（子树根）的统计量
        let vis = tree.visits[0];
        let wr = if vis > 0 { tree.wins[0] / vis as f64 } else { 0.0 };
        // 加入探索项以避免过早收敛
        let explore = UCB_C * ((branches * iters_per) as f64).ln().sqrt() / (vis.max(1) as f64).sqrt();
        (wr + explore, mx, my, vis)
    }).collect();

    // 选择综合得分最高的走法
    results.iter()
        .max_by(|(a, ..), (b, ..)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, x, y, _)| (*x, *y))
}


/// 公开入口：MCTS 搜索
pub fn find_best_move_mcts(
    board: &GameBoard,
    sz: usize,
    player: usize,
    depth: usize,  // 1~10，控制迭代次数
    eliminated: &[usize],
    max_players: usize,
    border_mode: BorderMode,
    cap_mode: CapMode,
) -> Option<(usize, usize)> {
    let iterations = depth.max(1) * MCTS_ITER_PER_DEPTH;
    mcts_search(board, sz, player, iterations, eliminated, max_players, border_mode, cap_mode, None)
}


/// 获取下一个活跃玩家（在 process_click 后调用）
pub fn next_live_player(board: &GameBoard, _sz: usize, current: usize, eliminated: &[usize], max_players: usize) -> usize {
    let alive: Vec<usize> = (0..max_players)
        .filter(|p| !eliminated.contains(p) && (*p == current || has_pieces(board, *p)))
        .collect();
    if alive.len() <= 1 { return current; }
    let idx = alive.iter().position(|&p| p == current).unwrap_or(0);
    alive[(idx + 1) % alive.len()]
}


/// 首步选中心：在不受「首子 12 格限制」的可用格里，挑离形状重心最近的一个。
///
/// 原来的写法先用 `(1..sz-1)` 排除最外圈，再按矩形中心算距离 —— 在口字棋盘
/// （只有外圈可用）上候选直接为空，只能落到兜底分支；在十字/长条上
/// 「矩形中心」也可能是个洞。改成直接对可用格求重心距离。
pub fn first_move_center(board: &GameBoard, sz: usize) -> Option<(usize, usize)> {
    let cols = board.first().map_or(sz, |r| r.len());
    let pick = |require_unrestricted: bool| -> Option<(usize, usize)> {
        let (row_c, col_c) = board_centroid(board);
        let mut best: Option<((usize, usize), f64)> = None;
        for i in 0..sz {
            for j in 0..cols {
                let c = &board[i][j];
                if c.owner.is_some() || c.blocked { continue; }
                if require_unrestricted && is_in_any_restricted_zone(board, i, j) { continue; }
                let d = (i as f64 - row_c).abs() + (j as f64 - col_c).abs();
                if best.map_or(true, |(_, bd)| d < bd) { best = Some(((i, j), d)); }
            }
        }
        best.map(|(p, _)| p)
    };
    pick(true).or_else(|| pick(false))
}


// ─── Public entry points ───

pub fn find_best_move(
    board: &GameBoard,
    sz: usize,
    player: usize,
    depth: usize,
    eliminated: &[usize],
    max_players: usize,
    game_count: u32,
    _first_move_pos: Option<[usize; 2]>,
    use_ml_eval: bool,
    border_mode: BorderMode,
    cap_mode: CapMode,
    user_random_scale: u32,
) -> Option<(usize, usize)> {
    let state = GameState {
        board: board.clone(),
        sz,
        player,
        ai_player: player,
        eliminated: eliminated.to_vec(),
        max_players,
        game_count,
        use_ml_eval,
        border_mode,
        cap_mode,
        user_random_scale,
    };
    state.run(depth)
}


// ─── 策略算法（纯启发式规则，无需搜索） ───

/// 统计 (i,j) 周围指定等级的对手棋子数量（回环模式邻居按边界模式计算）
pub fn count_opponent_level_around(board: &GameBoard, i: usize, j: usize, player: usize, level: u8, border_mode: BorderMode) -> i32 {
    let mut cnt = 0;
    for (ni, nj) in nbrs_with_mode(board, i, j, border_mode) {
        let nc = &board[ni][nj];
        if nc.owner.is_some() && nc.owner != Some(player) && nc.count == level {
            cnt += 1;
        }
    }
    cnt
}


/// 检查 (i,j) 周围是否有指定等级的对手棋子（回环模式邻居按边界模式计算）
pub fn has_opponent_level_near(board: &GameBoard, i: usize, j: usize, player: usize, level: u8, border_mode: BorderMode) -> bool {
    for (ni, nj) in nbrs_with_mode(board, i, j, border_mode) {
        let nc = &board[ni][nj];
        if nc.owner.is_some() && nc.owner != Some(player) && nc.count == level {
            return true;
        }
    }
    false
}


/// 统计 (i,j) 周围任意对手棋子数（回环模式邻居按边界模式计算）
pub fn count_any_opponent_around(board: &GameBoard, i: usize, j: usize, player: usize, border_mode: BorderMode) -> i32 {
    let mut cnt = 0;
    for (ni, nj) in nbrs_with_mode(board, i, j, border_mode) {
        let nc = &board[ni][nj];
        if nc.owner.is_some() && nc.owner != Some(player) {
            cnt += 1;
        }
    }
    cnt
}


/// 策略算法入口：纯启发式规则，无需搜索
/// 阈值感知：把 cap4 模式的“即将爆炸”判断（三三相接）迁移到
/// cap3（二二相接）与 cap5（四四相接）；回环模式上下左右判断与回环相符。
pub fn find_best_move_strategy(
    board: &GameBoard,
    sz: usize,
    player: usize,
    _eliminated: &[usize],
    _max_players: usize,
    _game_count: u32,
    _first_move_pos: Option<[usize; 2]>,
    border_mode: BorderMode,
    cap_mode: CapMode,
) -> Option<(usize, usize)> {
    let cols = board.first().map_or(sz, |r| r.len());
    // 收集己方棋子
    let mut mine: Vec<(usize, usize)> = Vec::new();
    for i in 0..sz {
        for j in 0..cols {
            if board[i][j].owner == Some(player) {
                mine.push((i, j));
            }
        }
    }

    // 首步：居中偏好（复用已有逻辑）
    if mine.is_empty() {
        return first_move_center(board, sz);
    }

    let mut rng = rand::thread_rng();
    // 回环边界下每个格子四个方向都能被打到，「贴边」不再是防守优势。
    // 原来无条件给边缘/角落加分，回环棋盘上等于在鼓励 AI 往一个并不更安全的位置堆子。
    let edge_safe = !matches!(border_mode, BorderMode::Wrap | BorderMode::Random);

    // 该格临界等级（再落一子即炸）：cap3→2、cap4→3、cap5→4；degrade 边界格子更低
    let crit_of = |i: usize, j: usize| crit_level(board, i, j, border_mode, cap_mode) as u8;

    // 1. 引爆临界棋子：己方 count == crit，优先选附近有对手 count == crit 的
    //    cap4：三三相接；cap3：二二相接；cap5：四四相接
    let crits: Vec<(usize, usize)> = mine.iter()
        .filter(|&&(i, j)| board[i][j].count == crit_of(i, j))
        .copied()
        .collect();
    if !crits.is_empty() {
        let mut best = Vec::new();
        let mut best_cnt = -1i32;
        for &(i, j) in &crits {
            let cnt = count_opponent_level_around(board, i, j, player, crit_of(i, j), border_mode);
            if cnt > best_cnt {
                best_cnt = cnt;
                best = vec![(i, j)];
            } else if cnt == best_cnt {
                best.push((i, j));
            }
        }
        if best_cnt >= 1 {
            return Some(best[rng.gen_range(0..best.len())]);
        }
    }

    // 2. 安全升级（count == crit-1，且附近没有对手临界棋子）
    let subcrits: Vec<(usize, usize)> = mine.iter()
        .filter(|&&(i, j)| board[i][j].count == crit_of(i, j).saturating_sub(1))
        .copied()
        .collect();
    let safe_sub: Vec<(usize, usize)> = subcrits.iter()
        .filter(|&&(i, j)| !has_opponent_level_near(board, i, j, player, crit_of(i, j), border_mode))
        .copied()
        .collect();
    if !safe_sub.is_empty() {
        let mut best = Vec::new();
        let mut best_score = f64::NEG_INFINITY;
        for &(i, j) in &safe_sub {
            let exp = exposure_count(board, i, j);
            let edge = if edge_safe && exp == 1 { 3.0 } else { 0.0 };
            let corner = if edge_safe && exp >= 2 { 5.0 } else { 0.0 };
            let near_any_opp = count_any_opponent_around(board, i, j, player, border_mode);
            let score = edge + corner - near_any_opp as f64 * 5.0 + rng.gen_range(0.0..1.0)*0.5;
            if score > best_score + 0.01 {
                best_score = score;
                best = vec![(i, j)];
            } else if (score - best_score).abs() < 0.01 {
                best.push((i, j));
            }
        }
        if !best.is_empty() {
            return Some(best[rng.gen_range(0..best.len())]);
        }
    }

    // 3. 更安全的升级（count == crit-2；cap3 下 crit-2=0 不存在，自动跳过）
    let sub2: Vec<(usize, usize)> = mine.iter()
        .filter(|&&(i, j)| board[i][j].count == crit_of(i, j).saturating_sub(2))
        .copied()
        .collect();
    if !sub2.is_empty() {
        let mut best = Vec::new();
        let mut best_score = f64::NEG_INFINITY;
        for &(i, j) in &sub2 {
            let near_opp = count_any_opponent_around(board, i, j, player, border_mode);
            let score = -near_opp as f64 * 3.0 + rng.gen_range(0.0..1.0)*0.5 * 2.0;
            if score > best_score + 0.01 {
                best_score = score;
                best = vec![(i, j)];
            } else if (score - best_score).abs() < 0.01 {
                best.push((i, j));
            }
        }
        if !best.is_empty() {
            return Some(best[rng.gen_range(0..best.len())]);
        }
    }

    // 4. 建立临界威胁（把 count == crit-1 升到 crit）
    //    cap4：二升三；cap3：一升二（安全）；cap5：三升四。靠近对手建立威胁。
    if !subcrits.is_empty() {
        let mut best = None;
        let mut best_score = f64::NEG_INFINITY;
        for &(i, j) in &subcrits {
            let near_opp = count_any_opponent_around(board, i, j, player, border_mode);
            let edge = if edge_safe && exposure_count(board, i, j) == 1 { 2.0 } else { 0.0 };
            let score = near_opp as f64 * 5.0 + edge + rng.gen_range(0.0..1.0)*0.5;
            if score > best_score + 0.01 {
                best_score = score;
                best = Some((i, j));
            }
        }
        if let Some(m) = best {
            return Some(m);
        }
    }

    // 5. 随机选一个可下的棋子（count < 本格阈值；cap3 下含临界 2，cap5 下含临界 4）
    let available: Vec<(usize, usize)> = mine.iter()
        .filter(|&&(i, j)| (board[i][j].count as u32) < cell_threshold(board, i, j, border_mode, cap_mode))
        .copied()
        .collect();
    if !available.is_empty() {
        return Some(available[rng.gen_range(0..available.len())]);
    }

    // 6. 保底：优先返回可下棋子；若棋盘存在 count>=阈值 的异常状态（如模拟器构造的
    //    非法初始局面），仍返回第一个己方棋子让 process_click 按爆炸处理，避免无棋可下
    mine.iter().copied()
        .find(|&(i, j)| (board[i][j].count as u32) < cell_threshold(board, i, j, border_mode, cap_mode))
        .or_else(|| mine.first().copied())
}




// ─── 自对弈数据生成 (XGBoost 训练用) ───

/// 玩家 AI 配置
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PlayerAiConfig {
    pub algorithm: String,
    pub depth: usize,
    #[serde(default = "default_true")]
    pub use_ml_eval: bool,
}


pub fn default_true() -> bool { true }


pub fn find_best_move_pvs(
    board: &GameBoard,
    sz: usize,
    player: usize,
    depth: usize,
    eliminated: &[usize],
    max_players: usize,
    game_count: u32,
    use_ml_eval: bool,
    border_mode: BorderMode,
    cap_mode: CapMode,
    user_random_scale: u32,
) -> Option<(usize, usize)> {
    let mut searcher = PvsSearcher::new(player, game_count, use_ml_eval, border_mode, cap_mode, user_random_scale);
    searcher.find_best(board, sz, player, max_players, eliminated, depth)
}


pub fn find_best_move_by_alg(
    board: &GameBoard,
    sz: usize,
    player: usize,
    config: &PlayerAiConfig,
    eliminated: &[usize],
    max_players: usize,
    game_count: u32,
    first_move_pos: Option<[usize; 2]>,
    border_mode: BorderMode,
    cap_mode: CapMode,
    user_random_scale: u32,
) -> Option<(usize, usize)> {
    match config.algorithm.as_str() {
        "alphabeta" => {
            find_best_move(board, sz, player, config.depth, eliminated, max_players, game_count, first_move_pos, config.use_ml_eval, border_mode, cap_mode, user_random_scale)
        }
        "pvs" => {
            find_best_move_pvs(board, sz, player, config.depth, eliminated, max_players, game_count, config.use_ml_eval, border_mode, cap_mode, user_random_scale)
        }
        "mcts" => {
            find_best_move_mcts(board, sz, player, config.depth, eliminated, max_players, border_mode, cap_mode)
        }
        _ => {
            find_best_move_strategy(board, sz, player, eliminated, max_players, game_count, first_move_pos, border_mode, cap_mode)
        }
    }
}

// ═══════ 异形棋盘 × 边界模式 ═══════
// 洞（blocked）在回环/反弹/降级下一律按"形状边界"处理：
// 回环绕过洞落到对侧第一个可用格，反弹把撞洞的能量弹回正对方向，
// 降级按形状外沿数决定阈值。默认边界保持不变 —— 棋子飞进洞仍然消失。
#[cfg(test)]
mod tests {
    use super::*;

    fn empty(rows: usize, cols: usize) -> GameBoard {
        vec![vec![Cell { owner: None, count: 0, th: None, blocked: false }; cols]; rows]
    }

    fn hole(b: &mut GameBoard, x: usize, y: usize) {
        b[x][y] = Cell { owner: None, count: 0, th: None, blocked: true };
    }

    fn put(b: &mut GameBoard, x: usize, y: usize, owner: usize, count: u8) {
        b[x][y] = Cell { owner: Some(owner), count, th: None, blocked: false };
    }

    #[test]
    fn wrap_target_skips_holes_and_wraps() {
        let mut b = empty(5, 5);
        // 从 (2,2) 向下：(3,2)、(4,2) 是洞，回绕到 (0,2) 也是洞，最后落在 (1,2)
        hole(&mut b, 3, 2);
        hole(&mut b, 4, 2);
        hole(&mut b, 0, 2);
        assert_eq!(wrap_target(&b, 2, 2, 1, 0), Some((1, 2)));
        // 向左：整列只有起点可用时不绕回自己
        let mut c = empty(3, 3);
        hole(&mut c, 0, 1);
        hole(&mut c, 2, 1);
        assert_eq!(wrap_target(&c, 1, 1, -1, 0), None);
    }

    #[test]
    fn wrap_explosion_crosses_board_edge() {
        let mut b = empty(5, 5);
        put(&mut b, 0, 0, 0, 3);   // 差一颗就炸
        process_click(&mut b, 5, 0, 0, 0, 2, BorderMode::Wrap, CapMode::Cap4, Some(1));
        // 上、左都出界：棋子应绕到对侧，而不是消失
        assert_eq!(b[4][0].owner, Some(0));
        assert_eq!(b[0][4].owner, Some(0));
        assert_eq!(b[1][0].owner, Some(0));
        assert_eq!(b[0][1].owner, Some(0));
    }

    #[test]
    fn bounce_off_hole_hits_opposite_side() {
        let mut b = empty(5, 5);
        hole(&mut b, 3, 2);        // 下方是洞：向下爆炸的能量应弹回上方
        put(&mut b, 2, 2, 0, 3);
        process_click(&mut b, 5, 2, 2, 0, 2, BorderMode::Bounce, CapMode::Cap4, Some(1));
        assert_eq!(b[1][2].owner, Some(0));
        assert_eq!(b[1][2].count, 2, "上方向：正常扩散 + 撞洞反弹 = 2 颗");
        assert_eq!(b[4][2].owner, None, "反弹不是穿过去");
        assert!(b[3][2].blocked && b[3][2].owner.is_none(), "洞保持为空");
        assert_eq!(b[2][1].count, 1);
        assert_eq!(b[2][3].count, 1);
    }

    #[test]
    fn bounce_rect_behaviour_unchanged() {
        let mut b = empty(5, 5);
        put(&mut b, 0, 2, 0, 3);
        process_click(&mut b, 5, 0, 2, 0, 2, BorderMode::Bounce, CapMode::Cap4, Some(1));
        assert_eq!(b[1][2].count, 2, "矩形边界：出界方向的能量仍弹到正对邻居");
        assert_eq!(b[0][1].count, 1);
        assert_eq!(b[0][3].count, 1);
    }

    #[test]
    fn degrade_matches_rect_edges() {
        let b = empty(5, 5);
        let deg = |x, y| cell_threshold(&b, x, y, BorderMode::Degrade, CapMode::Cap4);
        assert_eq!(deg(0, 0), 2, "角");
        assert_eq!(deg(0, 2), 3, "边");
        assert_eq!(deg(2, 2), 4, "内部");
    }

    #[test]
    fn degrade_follows_irregular_outline() {
        let mut b = empty(5, 5);
        hole(&mut b, 2, 1);
        let deg = |b: &GameBoard, x, y| cell_threshold(b, x, y, BorderMode::Degrade, CapMode::Cap4);
        assert_eq!(deg(&b, 2, 2), 3, "紧贴一个洞：算边");
        hole(&mut b, 1, 2);
        assert_eq!(deg(&b, 2, 2), 2, "同时贴两条形状边：算角");
        assert_eq!(deg(&b, 2, 3), 4, "远离形状边界的格子不受影响");
    }

    #[test]
    fn default_border_still_loses_pieces_in_holes() {
        let mut b = empty(5, 5);
        hole(&mut b, 1, 2);
        put(&mut b, 2, 2, 0, 3);
        process_click(&mut b, 5, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap4, Some(1));
        assert!(b[1][2].owner.is_none(), "默认边界：棋子飞进洞仍然消失");
        assert_eq!(b[2][1].count, 1);
        assert_eq!(b[2][3].count, 1);
        assert_eq!(b[3][2].count, 1);
    }

    /// 异形棋盘 + 三种边界各跑一局完整模拟：只要不卡死、不 panic、能走棋就算过。
    /// 规则正确性由上面几个用例保证，这里盯的是 AI 与连锁在真实对局里的稳定性。
    #[test]
    fn full_game_on_irregular_board_all_borders() {
        for bm in [BorderMode::Wrap, BorderMode::Bounce, BorderMode::Degrade] {
            // 5×5 口字：中间 3×3 挖空，只剩外圈一圈可用格
            let mut b = empty(5, 5);
            for x in 1..4 { for y in 1..4 { hole(&mut b, x, y); } }
            put(&mut b, 0, 0, 0, 0);   // 占位由首子逻辑接管，这里只需至少一格可用
            b[0][0] = Cell { owner: None, count: 0, th: None, blocked: false };
            put(&mut b, 0, 1, 0, 1);
            put(&mut b, 4, 3, 1, 1);
            let mut cfg = std::collections::HashMap::new();
            for p in 0..2 {
                cfg.insert(p.to_string(), serde_json::json!({
                    "algorithm": "strategy", "depth": 1, "useMlEval": false, "randomScale": 5,
                }));
            }
            let r = simulate_to_end(b.clone(), 5, 2, 0, vec![], bm, CapMode::Cap4, None, 0, cfg);
            assert!(!r.history.is_empty(), "{:?}：异形棋盘上应当能走棋", bm);
            let pieces: usize = r.board.iter().flatten().filter(|c| c.owner.is_some()).count();
            assert!(pieces <= r.board.iter().flatten().filter(|c| !c.blocked).count(),
                "{:?}：棋子数不可能超过可用格数", bm);
        }
    }

    #[test]
    fn ai_finds_legal_move_on_irregular_wrap_board() {
        let mut b = empty(5, 5);
        for &(x, y) in &[(0, 0), (0, 4), (4, 0), (4, 4), (0, 2), (4, 2)] {
            hole(&mut b, x, y);
        }
        put(&mut b, 2, 2, 0, 1);
        put(&mut b, 2, 3, 1, 1);
        let mv = find_best_move_strategy(&b, 5, 0, &[], 2, 10, None, BorderMode::Wrap, CapMode::Cap4);
        let (x, y) = mv.expect("异形棋盘上也要能给出走法");
        assert!(!b[x][y].blocked, "AI 不能把子下进洞里");
        assert_eq!(b[x][y].owner, Some(0), "有子时只能升级己方棋子");
    }
}

// ═══════ 评估函数与边界语义 ═══════
#[cfg(test)]
mod eval_tests {
    use super::*;

    fn empty(rows: usize, cols: usize) -> GameBoard {
        vec![vec![Cell { owner: None, count: 0, th: None, blocked: false }; cols]; rows]
    }

    fn put(b: &mut GameBoard, x: usize, y: usize, owner: usize, count: u8) {
        b[x][y] = Cell { owner: Some(owner), count, th: None, blocked: false };
    }

    fn hole(b: &mut GameBoard, x: usize, y: usize) {
        b[x][y] = Cell { owner: None, count: 0, th: None, blocked: true };
    }

    /// 回环边界下没有「中心更好」这回事：把同一个子放在中心或角落，位置分应当一致。
    /// 非回环边界则必须仍然偏好中心。
    #[test]
    fn position_bonus_ignores_edges_under_wrap() {
        let mut center = empty(7, 7);
        put(&mut center, 3, 3, 0, 1);
        let mut corner = empty(7, 7);
        put(&mut corner, 0, 0, 0, 1);

        let c_wrap = eval_board_improved(&center, 0, 10, BorderMode::Wrap, CapMode::Cap4, 0);
        let k_wrap = eval_board_improved(&corner, 0, 10, BorderMode::Wrap, CapMode::Cap4, 0);
        assert_eq!(c_wrap, k_wrap, "回环下中心与角落不应有位置差");

        let c_def = eval_board_improved(&center, 0, 10, BorderMode::Default, CapMode::Cap4, 0);
        let k_def = eval_board_improved(&corner, 0, 10, BorderMode::Default, CapMode::Cap4, 0);
        assert!(c_def > k_def, "默认边界下中心仍应优于角落");
    }

    /// 贴身的价值要按点数区分：临界棋子贴着对手是势能，1 点棋子贴着是送子。
    #[test]
    fn adjacency_scored_by_count() {
        // 己方 3 点（cap4 临界）贴着对手 1 点
        let mut good = empty(5, 5);
        put(&mut good, 2, 2, 0, 3);
        put(&mut good, 2, 3, 1, 1);
        // 换成己方 1 点贴着对手 1 点
        let mut bad = empty(5, 5);
        put(&mut bad, 2, 2, 0, 1);
        put(&mut bad, 2, 3, 1, 1);

        let g = eval_board_improved(&good, 0, 10, BorderMode::Default, CapMode::Cap4, 0);
        let b = eval_board_improved(&bad, 0, 10, BorderMode::Default, CapMode::Cap4, 0);
        assert!(g > b, "临界棋子贴身应当比 1 点棋子贴身得分更高（{g} vs {b}）");
    }

    /// 降级边界下边缘格阈值更低，手写评估必须按格算临界等级。
    #[test]
    fn handcraft_uses_per_cell_threshold() {
        // 角格阈值 2（临界 1）：1 点就是临界；中心格阈值 4（临界 3）
        let mut corner = empty(5, 5);
        put(&mut corner, 0, 0, 0, 1);
        let mut middle = empty(5, 5);
        put(&mut middle, 2, 2, 0, 1);

        let c = eval_board_handcraft(&corner, 0, 10, BorderMode::Degrade, CapMode::Cap4, 0);
        let m = eval_board_handcraft(&middle, 0, 10, BorderMode::Degrade, CapMode::Cap4, 0);
        // 角上那颗已经临界（chain_threat 高），但位置分低；只要两者不等就说明阈值生效了
        assert_ne!(c, m, "降级边界下角格与中格的临界等级不同，评分不应相同");
    }

    /// 形状重心：口字棋盘的几何中心是洞，重心必须落在外框上。
    #[test]
    fn centroid_ignores_holes() {
        let mut b = empty(5, 5);
        for x in 1..4 { for y in 1..4 { hole(&mut b, x, y); } }
        let (r, c) = board_centroid(&b);
        // 只有外圈可用，重心仍在几何中心 (2,2)，但那个点是洞
        assert!((r - 2.0).abs() < 0.01 && (c - 2.0).abs() < 0.01);
        assert!(b[2][2].blocked);
    }

    /// 首步选中心：口字棋盘只有外圈可用，旧实现会因 (1..sz-1) 过滤掉全部候选而走兜底，
    /// 现在应当稳定返回一个合法可用格。
    #[test]
    fn first_move_on_ring_board() {
        let mut b = empty(7, 7);
        for x in 2..5 { for y in 2..5 { hole(&mut b, x, y); } }
        let mv = first_move_center(&b, 7).expect("口字棋盘上应当能找到首步");
        assert!(!b[mv.0][mv.1].blocked, "首步不能落在洞里");
        assert!(b[mv.0][mv.1].owner.is_none());
    }
}
