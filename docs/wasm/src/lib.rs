// Copyright (c) 2026 ywnh1
// SPDX-License-Identifier: MIT
//
// 连锁棋 PWA 游戏引擎（WASM）。
// 由 tauri/src-tauri/src/lib.rs 提取纯逻辑部分生成：
//  - 游戏规则（process_click 系列）
//  - AI 引擎（Alpha-Beta / PVS / MCTS / 策略搜索）
//  - XGBoost 机器学习评估（模型内嵌）
// Tauri 专用代码（存储、更新、对话框）不在此 crate，由 pwa/engine.js 负责。
#![allow(clippy::too_many_arguments)]

use std::collections::{HashMap, HashSet, VecDeque};
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use rand::Rng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use alpha_beta_pruning::{AlphaBeta, Grade};

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
}

pub type GameBoard = Vec<Vec<Cell>>;

// ─── History types ───

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChainStatsPlayer {
    pub triggered: u32,
    pub max_chain: u32,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MaxChain {
    pub player: Option<usize>,
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
    pub winner: Option<usize>,
    pub color_names: Vec<String>,
    pub chain_stats: std::collections::HashMap<String, ChainStatsPlayer>,
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

// ─── State ───

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
let mut b = board.clone();
let mut elim = eliminated.clone();
let mut killed_by: Vec<(usize, usize)> = Vec::new();
let mut chain_stats: std::collections::HashMap<String, ChainStatsPlayer> = std::collections::HashMap::new();
let mut max_chain = MaxChain { player: None, length: 0 };
let mut history: Vec<TurnHistory> = Vec::new();
let mut cur = cur_player;
let mut gc = game_count;
let mut no_move_round = 0usize;
let max_steps = (size * size * 4 * max_players).max(1000) as usize;
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

const FEAT_DIM: usize = 18;

#[derive(Clone, Deserialize)]
#[allow(dead_code)]
struct XgbTreeModel {
    base_weights: Vec<f32>, default_left: Vec<i32>,
    left_children: Vec<i32>, right_children: Vec<i32>,
    split_conditions: Vec<f32>, split_indices: Vec<u32>,
    tree_param: XgbTreeParam,
}
#[derive(Clone, Deserialize)]
#[allow(dead_code)]
struct XgbTreeParam { num_nodes: String, }
#[derive(Clone, Deserialize)]
struct XgbGradientBoosterModel { trees: Vec<XgbTreeModel>, }
#[derive(Clone, Deserialize)]
struct XgbGradientBooster { model: XgbGradientBoosterModel, }
#[derive(Clone, Deserialize)]
struct XgbLearnerModelParam { base_score: String, }
#[derive(Clone, Deserialize)]
struct XgbLearner { gradient_booster: XgbGradientBooster, learner_model_param: XgbLearnerModelParam, }
#[derive(Clone, Deserialize)]
struct XgbModel { learner: XgbLearner, }

pub struct XgbNode { pub split_feat: usize, pub split_cond: f32, pub left: i32, pub right: i32, pub leaf: f32, pub is_leaf: bool, }
pub struct XGBoostEngine { pub trees: Vec<Vec<XgbNode>>, base_score: f32, }

use std::sync::OnceLock;

impl XGBoostEngine {
    #[allow(dead_code)]
    #[cfg(not(target_arch = "wasm32"))]
    fn load(path: &str) -> Result<Self, String> {
        let c = fs::read_to_string(path).map_err(|e| format!("读模型: {}", e))?;
        Self::load_from_str(&c)
    }

    fn load_from_str(json: &str) -> Result<Self, String> {
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

    fn predict(&self, feats: &[f32; FEAT_DIM]) -> (f32, f32) {
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
fn xgb_engine() -> &'static XGBoostEngine {
    static ENG: OnceLock<XGBoostEngine> = OnceLock::new();
    ENG.get_or_init(|| {
        XGBoostEngine::load_from_str(include_str!("../xgb_model_board.json")).expect("加载 xgb_model_board.json 失败")
    })
}

/// 改进版特征提取：用 eval_board_improved 中间值做特征
/// 兼容所有棋盘大小（5-19）、玩家人数（2-10）、边界模式与爆炸阈值模式
/// 18 维：前 16 维保持与旧模型一致（c3/c2 语义不变），新增 [16]/[17] 为阈值感知的临界棋子占比
pub fn extract_features_improved(board: &GameBoard, cur: usize, max_players: usize, border_mode: BorderMode, cap_mode: CapMode) -> [f32; FEAT_DIM] {
    let sz = board.len();
    let total_cells = (sz * sz) as f32;
    let cx = (sz as f32 - 1.0) * 0.5;

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
        let dist_center = ((i as f32 - cx).abs() * 0.5) as i32;
        for (j, c) in row.iter().enumerate() {
            if let Some(owner) = c.owner {
                total_pieces += 1;
                let d = dist_center + ((j as f32 - cx).abs() * 0.5) as i32;
                let pos_val = 4i32.saturating_sub(d).max(0);
                let crit = crit_level(i, j, sz, border_mode, cap_mode) as i32;
                if owner == cur {
                    my_score += c.count as i32;
                    my_territory += 1;
                    my_pos_bonus += pos_val;
                    my_cdist += (i as f32 - cx).abs() + (j as f32 - cx).abs();
                    my_pieces += 1;
                    if c.count == 2 { my_c2 += 1; }
                    if c.count >= 3 { my_c3 += 1; }
                    // 阈值感知威胁：临界等级高威胁，临界-1 中威胁
                    if (c.count as i32) >= crit { my_chain_threat += (c.count as i32) * 5; my_crit += 1; }
                    else if (c.count as i32) >= crit - 1 { my_chain_threat += 2; }
                    for &(ni, nj) in &nbrs_with_mode(i, j, sz, border_mode) {
                        let nc = &board[ni][nj];
                        if nc.owner.is_some() && nc.owner != Some(cur) {
                            my_threat_prox += c.count as i32;
                        }
                    }
                } else {
                    opp_score += c.count as i32;
                    opp_territory += 1;
                    opp_pos_bonus += pos_val;
                    opp_cdist += (i as f32 - cx).abs() + (j as f32 - cx).abs();
                    opp_pieces += 1;
                    if c.count >= 3 { opp_c3 += 1; }
                    if (c.count as i32) >= crit { opp_chain_threat += (c.count as i32) * 5; opp_crit += 1; }
                    else if (c.count as i32) >= crit - 1 { opp_chain_threat += 2; }
                    for &(ni, nj) in &nbrs_with_mode(i, j, sz, border_mode) {
                        let nc = &board[ni][nj];
                        if nc.owner == Some(cur) {
                            opp_threat_prox += c.count as i32;
                        }
                    }
                }
            }
        }
    }
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
#[cfg(not(target_arch = "wasm32"))]
pub fn get_xgb_engine_for_test(path: &str) -> XGBoostEngine {
    XGBoostEngine::load(path).expect("加载模型失败")
}
pub fn xgb_predict(engine: &XGBoostEngine, feats: &[f32; FEAT_DIM]) -> (f32, f32) {
    engine.predict(feats)
}

/// 增强版评估函数：在原手写评估基础上加入位置权重、邻居威胁、爆发势能
/// 深度优化——单次遍历，零额外 Vec 分配
pub fn eval_board_improved(board: &GameBoard, player: usize, game_count: u32, border_mode: BorderMode, cap_mode: CapMode, user_random_scale: u32) -> i32 {
    let sz = board.len();
    let cx = (sz as f64 - 1.0) * 0.5;
    let mut my_score = 0i32;
    let mut opp_score = 0i32;
    let mut my_territory = 0i32;
    let mut opp_territory = 0i32;
    let mut my_chain_threat = 0i32;
    let mut opp_chain_threat = 0i32;
    let mut my_pos_bonus = 0i32;     // 位置优势（靠近中心）
    let mut opp_pos_bonus = 0i32;
    let mut my_threat_prox = 0i32;   // 己方棋子靠近对手=威胁力
    let mut opp_threat_prox = 0i32;  // 对手棋子靠近己方=危险度

    for i in 0..sz {
        let dist_center = ((i as f64 - cx).abs() * 0.5) as i32;
        for j in 0..sz {
            let cell = &board[i][j];
            let d = dist_center + ((j as f64 - cx).abs() * 0.5) as i32;
            let pos_val = 4i32.saturating_sub(d).max(0); // 中心~4, 角落~0

            match cell.owner {
                Some(p) if p == player => {
                    my_score += cell.count as i32;
                    my_territory += 1;
                    my_pos_bonus += pos_val;
                    let crit = crit_level(i, j, sz, border_mode, cap_mode) as i32;
                    if (cell.count as i32) >= crit { my_chain_threat += (cell.count as i32) * 5; }
                    else if (cell.count as i32) >= crit - 1 { my_chain_threat += 2; }
                    // 邻居对手计数：己方高级棋子靠近对手 = 爆发势能
                    let nbrs = nbrs_with_mode(i, j, sz, border_mode);
                    for &(ni, nj) in &nbrs {
                        let nc = &board[ni][nj];
                        if nc.owner.is_some() && nc.owner != Some(player) {
                            my_threat_prox += cell.count as i32;
                        }
                    }
                }
                Some(_) => {
                    opp_score += cell.count as i32;
                    opp_territory += 1;
                    opp_pos_bonus += pos_val;
                    let crit = crit_level(i, j, sz, border_mode, cap_mode) as i32;
                    if (cell.count as i32) >= crit { opp_chain_threat += (cell.count as i32) * 5; }
                    else if (cell.count as i32) >= crit - 1 { opp_chain_threat += 2; }
                    let nbrs = nbrs_with_mode(i, j, sz, border_mode);
                    for &(ni, nj) in &nbrs {
                        let nc = &board[ni][nj];
                        if nc.owner == Some(player) {
                            opp_threat_prox += cell.count as i32;
                        }
                    }
                }
                None => {}
            }
        }
    }

    let base = (my_score - opp_score) * 2
        + (my_territory - opp_territory)
        + (my_chain_threat - opp_chain_threat)  // chain_threat 已经 *5/*2
        + (my_pos_bonus - opp_pos_bonus) * 2    // 位置优势
        + (my_threat_prox - opp_threat_prox) * 3; // 威胁势能权重最高

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

fn eval_board_ml(board: &GameBoard, player: usize, game_count: u32, border_mode: BorderMode, cap_mode: CapMode, user_random_scale: u32) -> i32 {
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
fn max_players_for(board: &GameBoard) -> usize {
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
fn nbrs(i: usize, j: usize, sz: usize) -> Vec<(usize, usize)> {
    let mut r = Vec::with_capacity(4);
    if i > 0 { r.push((i - 1, j)); }
    if i + 1 < sz { r.push((i + 1, j)); }
    if j > 0 { r.push((i, j - 1)); }
    if j + 1 < sz { r.push((i, j + 1)); }
    r
}

/// 回环边界邻居：四个方向全部回环
fn nbrs_wrap(i: usize, j: usize, sz: usize) -> Vec<(usize, usize)> {
    let up = if i == 0 { sz - 1 } else { i - 1 };
    let down = if i + 1 >= sz { 0 } else { i + 1 };
    let left = if j == 0 { sz - 1 } else { j - 1 };
    let right = if j + 1 >= sz { 0 } else { j + 1 };
    vec![(up, j), (down, j), (i, left), (i, right)]
}

/// 根据边界模式选择邻居函数
fn nbrs_with_mode(i: usize, j: usize, sz: usize, border_mode: BorderMode) -> Vec<(usize, usize)> {
    match border_mode {
        BorderMode::Wrap => nbrs_wrap(i, j, sz),
        _ => nbrs(i, j, sz),
    }
}


#[inline]
pub fn has_pieces(board: &GameBoard, player: usize) -> bool {
    board.iter().flatten().any(|c| c.owner == Some(player))
}

fn is_in_first_move_restricted(x: usize, y: usize, fx: usize, fy: usize) -> bool {
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

fn is_in_any_restricted_zone(board: &GameBoard, sz: usize, x: usize, y: usize) -> bool {
    for i in 0..sz {
        for j in 0..sz {
            if board[i][j].owner.is_some() && is_in_first_move_restricted(x, y, i, j) {
                return true;
            }
        }
    }
    false
}

/// 计算格子的爆炸阈值：cap3/cap5 固定；随机每步随机 3/4/5；默认(4)保留降级边界位置修正
fn explosion_threshold(_cell: &Cell, x: usize, y: usize, sz: usize, border_mode: BorderMode, cap_mode: CapMode, rng: &mut Box<dyn rand::RngCore>) -> u32 {
    match cap_mode {
        CapMode::Cap3 => 3,
        CapMode::Cap5 => 5,
        CapMode::Random => 3 + rng.gen_range(0..3),
        CapMode::Cap4 => match border_mode {
            BorderMode::Degrade => {
                let on_corner = (x == 0 || x == sz - 1) && (y == 0 || y == sz - 1);
                let on_edge = x == 0 || x == sz - 1 || y == 0 || y == sz - 1;
                if on_corner { 2 } else if on_edge { 3 } else { 4 }
            }
            _ => 4,
        },
    }
}

/// 格子的爆炸阈值（无 RNG 版本，供 AI 走法生成/评估/排序使用）。
/// cap3/cap5 固定；cap4 默认 4、degrade 边界位置修正；random 模式搜索按中间值 4 处理。
#[inline]
fn cell_threshold(x: usize, y: usize, sz: usize, border_mode: BorderMode, cap_mode: CapMode) -> u32 {
    match cap_mode {
        CapMode::Cap3 => 3,
        CapMode::Cap5 => 5,
        CapMode::Random => 4,
        CapMode::Cap4 => match border_mode {
            BorderMode::Degrade => {
                let on_corner = (x == 0 || x == sz - 1) && (y == 0 || y == sz - 1);
                let on_edge = x == 0 || x == sz - 1 || y == 0 || y == sz - 1;
                if on_corner { 2 } else if on_edge { 3 } else { 4 }
            }
            _ => 4,
        },
    }
}

/// 己方棋子的“临界等级”：达到该等级后再落一子即爆炸（阈值 - 1）。
/// cap3→2、cap4→3、cap5→4；degrade 边界格子按位置修正。
#[inline]
fn crit_level(x: usize, y: usize, sz: usize, border_mode: BorderMode, cap_mode: CapMode) -> u32 {
    cell_threshold(x, y, sz, border_mode, cap_mode) - 1
}

fn process_click_with_killer_inner(
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
    {
        let cell = &mut board[x][y];
        if cell.owner.is_none() {
            cell.owner = Some(player);
            // 首子等级 = 阈值 n-1（临界态：再落一子即炸）；其余落子 +1
            let th = explosion_threshold(cell, x, y, sz, border_mode, cap_mode, &mut rng);
            cell.count = if is_first { th.saturating_sub(1) as u8 } else { 1 };
        } else if cell.owner == Some(player) {
            cell.count += 1;
        } else {
            return (vec![], 0, vec![], steps, exploded);
        }
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
        let threshold = explosion_threshold(&board[cx][cy], cx, cy, sz, eff_bm, cap_mode, &mut rng);

        if board[cx][cy].count as u32 >= threshold {
            let owner = board[cx][cy].owner.unwrap();
            board[cx][cy].count = 0;
            board[cx][cy].owner = None;
            chain_count += 1;

            // ── 按有效边界模式生成扩散目标；cap3/cap5 的随机特殊格对所有边界模式生效 ──
            let (targets, bounce_extra): (Vec<(usize, usize)>, Vec<(usize, usize)>) = match eff_bm {
                BorderMode::Wrap => {
                    // 回环边界：向四个方向爆炸，边界处回环到对面
                    let up = if cx == 0 { sz - 1 } else { cx - 1 };
                    let down = if cx + 1 >= sz { 0 } else { cx + 1 };
                    let left = if cy == 0 { sz - 1 } else { cy - 1 };
                    let right = if cy + 1 >= sz { 0 } else { cy + 1 };
                    (vec![(up, cy), (down, cy), (cx, left), (cx, right)], vec![])
                }
                BorderMode::Bounce => {
                    // 反弹边界：出界的爆炸能量反弹到正对的邻居上
                    // 方向排列：[上, 下, 左, 右]，方向对：0↔1 (上下)，2↔3 (左右)
                    let dirs = [
                        (cx.wrapping_sub(1), cy),
                        (cx + 1, cy),
                        (cx, cy.wrapping_sub(1)),
                        (cx, cy + 1),
                    ];
                    let opposite = [1usize, 0, 3, 2];
                    let mut targets = Vec::with_capacity(4);
                    for &(nx, ny) in &dirs {
                        if nx < sz && ny < sz {
                            targets.push((nx, ny));
                        }
                    }
                    if targets.is_empty() {
                        (targets, vec![])
                    } else {
                        // 第二遍：出界方向的能量反弹到正对方向（额外 +1）
                        let mut extra = Vec::with_capacity(4);
                        for (i, &(nx, ny)) in dirs.iter().enumerate() {
                            if nx >= sz || ny >= sz {
                                let (ox, oy) = dirs[opposite[i]];
                                if ox < sz && oy < sz {
                                    extra.push((ox, oy));
                                }
                            }
                        }
                        (targets, extra)
                    }
                }
                _ => (nbrs(cx, cy, sz), vec![]),  // 默认/降级：标准邻居扩散
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
                if !is_special {
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

pub fn eval_board_handcraft(board: &GameBoard, player: usize, game_count: u32, _border_mode: BorderMode, cap_mode: CapMode, user_random_scale: u32) -> i32 {
    let mut my_score = 0i32;
    let mut opp_score = 0i32;
    let mut my_territory = 0i32;
    let mut opp_territory = 0i32;
    let mut my_chain_threat = 0i32;
    let mut opp_chain_threat = 0i32;

    // 阈值感知的主临界等级（cap3→2、cap4→3、cap5→4；random 取中值 3）
    let crit = match cap_mode {
        CapMode::Cap3 => 2,
        CapMode::Cap5 => 4,
        CapMode::Random => 3,
        CapMode::Cap4 => 3,
    } as i32;

    for row in board {
        for cell in row {
            match cell.owner {
                Some(p) if p == player => {
                    my_score += cell.count as i32;
                    my_territory += 1;
                    if (cell.count as i32) >= crit { my_chain_threat += (cell.count as i32) * 4; }
                    else if (cell.count as i32) >= crit - 1 { my_chain_threat += 1; }
                }
                Some(_) => {
                    opp_score += cell.count as i32;
                    opp_territory += 1;
                    if (cell.count as i32) >= crit { opp_chain_threat += (cell.count as i32) * 4; }
                    else if (cell.count as i32) >= crit - 1 { opp_chain_threat += 1; }
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
    let has_p = has_pieces(board, player);
    let mut moves = Vec::new();
    for i in 0..sz {
        for j in 0..sz {
            let c = &board[i][j];
            if has_p {
                // 阈值感知：只有 count < 本格阈值 的棋子可以落子（cap5 下 count==4 的引爆动作合法）
                if c.owner == Some(player) && (c.count as u32) < cell_threshold(i, j, sz, border_mode, cap_mode) {
                    moves.push((i, j));
                }
            } else {
                if c.owner.is_none() && !is_in_any_restricted_zone(board, sz, i, j) {
                    moves.push((i, j));
                }
            }
        }
    }
    moves
}

fn order_moves(moves: Vec<(usize, usize)>, board: &GameBoard, sz: usize, player: usize, border_mode: BorderMode, cap_mode: CapMode) -> Vec<(usize, usize)> {
    let mut scored: Vec<(i32, (usize, usize))> = moves
        .into_iter()
        .map(|(i, j)| {
            let c = &board[i][j];
            let mut score = c.count as i32 * 10;
            // 阈值感知：临界等级（再落一子即炸）的走法优先
            if (c.count as u32) >= crit_level(i, j, sz, border_mode, cap_mode) {
                score += 100;
            }
            // 回环模式：邻居按边界模式计算（棋盘最上格的上方是最后一行）
            let near_opp: i32 = nbrs_with_mode(i, j, sz, border_mode)
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
struct GameState {
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
            let mut candidates: Vec<(usize, usize)> = (1..self.sz - 1)
                .flat_map(|i| (1..self.sz - 1).filter_map(move |j| {
                    if self.board[i][j].owner.is_none() && !is_in_any_restricted_zone(&self.board, self.sz, i, j) {
                        Some((i, j))
                    } else { None }
                })).collect();
            if candidates.is_empty() {
                for i in 0..self.sz { for j in 0..self.sz {
                    if self.board[i][j].owner.is_none() && !is_in_any_restricted_zone(&self.board, self.sz, i, j) {
                        candidates.push((i, j));
                    }
                }}
            }
            if candidates.is_empty() {
                for i in 0..self.sz { for j in 0..self.sz {
                    if self.board[i][j].owner.is_none() { candidates.push((i, j)); }
                }}
            }
            if candidates.is_empty() { return None; }
            let cx = self.sz as f64 / 2.0 - 0.5;
            return candidates.into_iter()
                .min_by(|&(i1, j1), &(i2, j2)| {
                    let d1 = (i1 as f64 - cx).abs() + (j1 as f64 - cx).abs();
                    let d2 = (i2 as f64 - cx).abs() + (j2 as f64 - cx).abs();
                    d1.partial_cmp(&d2).unwrap()
                });
        }

        // 走法排序 + 限制前10（阈值感知 + 回环邻居）
        let ordered = order_moves(all_moves, &self.board, self.sz, self.player, self.border_mode, self.cap_mode);
        let max_eval = ordered.len().min(10);
        if max_eval == 0 { return None; }

        // Rayon 并行根搜索（利用 crate 的 Grade + AlphaBeta trait）
        ordered[..max_eval].iter()
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
        moves = order_moves(moves, &self.board, self.sz, self.player, self.border_mode, self.cap_mode);
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

const PVS_MAX_DEPTH: usize = 64;
const PVS_HISTORY_SIZE: usize = 40;

/// 紧凑 eliminated bitset（最多支持 32 位玩家）
#[derive(Clone, Copy)]
struct ElimSet(u32);

impl ElimSet {
    fn from_slice(s: &[usize]) -> Self {
        let mut e = Self(0);
        for &p in s { e.0 |= 1u32 << (p as u32); }
        e
    }
    fn add(&mut self, p: usize) { self.0 |= 1u32 << (p as u32); }
    fn contains(&self, p: usize) -> bool { (self.0 >> (p as u32)) & 1 != 0 }
}

struct PvsSearcher {
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
    fn new(ai_player: usize, game_count: u32, use_ml_eval: bool, border_mode: BorderMode, cap_mode: CapMode, user_random_scale: u32) -> Self {
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
    fn get_moves_ordered(&self, board: &GameBoard, sz: usize, player: usize, depth: usize) -> Vec<(usize, usize)> {
        let has_p = has_pieces(board, player);
        let k0 = self.killers[0][depth];
        let k1 = self.killers[1][depth];
        // 预分配
        let cap = sz * sz;
        let mut moves: Vec<(i64, (usize, usize))> = Vec::with_capacity(cap);

        if has_p {
            for i in 0..sz { for j in 0..sz {
                let c = &board[i][j];
                // 阈值感知：cap5 下 count==4 的引爆走法合法且关键
                if c.owner == Some(player) && (c.count as u32) < cell_threshold(i, j, sz, self.border_mode, self.cap_mode) {
                    let mut score = c.count as i64 * 10;
                    if (c.count as u32) >= crit_level(i, j, sz, self.border_mode, self.cap_mode) { score += 100; }
                    // 邻居对手分数（回环模式用 nbrs_with_mode）
                    self.add_neighbor_scores(&mut score, board, sz, i, j, player);
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
            for i in 0..sz { for j in 0..sz {
                if board[i][j].owner.is_none() && !is_in_any_restricted_zone(board, sz, i, j) {
                    let mut score = 0i64;
                    // 邻居对手分数（回环模式用 nbrs_with_mode）
                    self.add_neighbor_scores(&mut score, board, sz, i, j, player);
                    moves.push((score, (i, j)));
                }
            }}
        }
        moves.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        moves.into_iter().map(|(_, m)| m).collect()
    }

    /// 邻居对手分数：回环模式按边界模式取邻居，其余模式手动展开避免 Vec 分配
    fn add_neighbor_scores(&self, score: &mut i64, board: &GameBoard, sz: usize, i: usize, j: usize, player: usize) {
        match self.border_mode {
            BorderMode::Wrap => {
                for &(ni, nj) in &nbrs_wrap(i, j, sz) {
                    let nc = &board[ni][nj];
                    if nc.owner.is_some() && nc.owner != Some(player) { *score += nc.count as i64 * 5; }
                }
            }
            _ => {
                if i > 0 { let nc = &board[i-1][j]; if nc.owner.is_some() && nc.owner != Some(player) { *score += nc.count as i64 * 5; } }
                if i + 1 < sz { let nc = &board[i+1][j]; if nc.owner.is_some() && nc.owner != Some(player) { *score += nc.count as i64 * 5; } }
                if j > 0 { let nc = &board[i][j-1]; if nc.owner.is_some() && nc.owner != Some(player) { *score += nc.count as i64 * 5; } }
                if j + 1 < sz { let nc = &board[i][j+1]; if nc.owner.is_some() && nc.owner != Some(player) { *score += nc.count as i64 * 5; } }
            }
        }
    }

    /// QSearch: 仅搜索爆炸性走法，使用 bitset 加速 eliminated 判断
    fn quiescence(
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
        for i in 0..sz { for j in 0..sz {
            let c = &board[i][j];
            let on_corner_deg = (i == 0 || i == sz-1) && (j == 0 || j == sz-1);
            let on_edge_deg = i == 0 || i == sz-1 || j == 0 || j == sz-1;
            let min_explosive = match self.cap_mode {
                CapMode::Cap3 => 1,
                CapMode::Cap5 => 3,
                CapMode::Random => 2,
                CapMode::Cap4 => match self.border_mode {
                    BorderMode::Degrade => {
                        if on_corner_deg { 1 } else if on_edge_deg { 2 } else { 3 }
                    }
                    _ => 3,
                },
            };
            if c.owner == Some(player) && c.count >= min_explosive && (c.count as u32) < cell_threshold(i, j, sz, self.border_mode, self.cap_mode) {
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

    fn pvs(
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

    fn find_best(
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
        let results: Vec<_> = ordered[..root_limit]
            .iter()
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
fn next_live_player_es(board: &GameBoard, _sz: usize, current: usize, elim: ElimSet, max_players: usize) -> usize {
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
struct XorShift(u64);

impl XorShift {
    fn seed(seed: u64) -> Self { Self(if seed == 0 { 1 } else { seed }) }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn next_usize(&mut self, range: usize) -> usize {
        if range <= 1 { return 0; }
        (self.next_u64() as usize) % range
    }
}

/// MCTS 常量
const UCB_C: f64 = 2.0;
const MCTS_ITER_PER_DEPTH: usize = 400; // iterations = depth * 400（原 800 耗时过长，减半仍保留深度语义）
const MCTS_PLAYOUT_MAX: usize = 24; // 模拟步数上限（原 40：链式大棋盘下是主要耗时，减到 24 兼顾速度与评估质量）
const MCTS_TREE_MAX_NODES: usize = 2000; // 每棵树最大节点数（防内存暴涨）

/// ─── MCTS 树节点（扁平向量存储） ───

/// 子边：一个走法 + 展开后的子节点索引
#[derive(Clone)]
struct MctsEdge {
    x: usize,
    y: usize,
    child: Option<usize>,
}

/// MCTS 搜索树
struct MctsTree {
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
    fn new(board: GameBoard, eliminated: Vec<usize>, player: usize, sz: usize, max_players: usize, ai_player: usize, max_nodes: usize, border_mode: BorderMode, cap_mode: CapMode) -> Self {
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
    fn add_node(&mut self, board: GameBoard, eliminated: Vec<usize>, player: usize) -> Option<usize> {
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
    fn untried_moves(&self, node: usize) -> Vec<(usize, usize)> {
        let all = get_moves(&self.boards[node], self.sz, self.players[node], None, self.border_mode, self.cap_mode);
        let kids = &self.children[node];
        // children 通常很小，线性扫描比 HashSet 更快（避免哈希分配）
        all.into_iter().filter(|m| !kids.iter().any(|e| e.x == m.0 && e.y == m.1)).collect()
    }

    /// 节点是否终局
    fn is_terminal(&self, node: usize) -> bool {
        let alive: Vec<usize> = (0..self.max_players)
            .filter(|p| !self.eliminateds[node].contains(p) && has_pieces(&self.boards[node], *p))
            .collect();
        alive.len() <= 1
    }

    /// UCB1 公式
    fn ucb(&self, parent: usize, child: usize) -> f64 {
        let pv = self.visits[parent] as f64;
        let cv = self.visits[child] as f64;
        if cv == 0.0 { return f64::MAX; }
        self.wins[child] / cv + UCB_C * (pv.ln() / cv).sqrt()
    }

    /// 用 UCB1 选择最佳已展开子节点
    fn select_child(&self, node: usize) -> Option<(usize, usize, usize)> {
        let kids: Vec<_> = self.children[node].iter()
            .filter_map(|e| e.child.map(|c| (e.x, e.y, c)))
            .collect();
        if kids.is_empty() { return None; }
        kids.into_iter()
            .max_by(|&(_, _, a), &(_, _, b)| self.ucb(node, a).partial_cmp(&self.ucb(node, b)).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// 展开一个节点：从已算好的 untried 中选一个（依 order_moves 排序），创建子节点
    /// 由 iterate 传入 untried，避免 select 阶段重复计算合法走法
    fn expand_with(&mut self, node: usize, mut untried: Vec<(usize, usize)>) -> Option<usize> {
        if self.visits.len() >= self.max_nodes { return None; }
        if untried.is_empty() { return None; }

        // 用 order_moves 排序后，优先展开最有希望的走法（阈值感知 + 回环邻居）
        untried = order_moves(untried, &self.boards[node], self.sz, self.players[node], self.border_mode, self.cap_mode);
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
    fn iterate(&mut self, rng: &mut XorShift) {
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
fn mcts_playout(
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
fn mcts_search(
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
    let ordered = order_moves(all_moves, board, sz, ai_player, border_mode, cap_mode);
    let branches = ordered.len().min(15);
    let iters_per = (iterations / branches).max(50);
    let max_nodes = MCTS_TREE_MAX_NODES / branches.max(1);

    // ─── Root-parallel MCTS ───
    // 每个根级候选走法获得独立的搜索树，在子树内执行完整 MCTS
    let results: Vec<_> = ordered[..branches].iter().map(|&(mx, my)| {
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
fn next_live_player(board: &GameBoard, _sz: usize, current: usize, eliminated: &[usize], max_players: usize) -> usize {
    let alive: Vec<usize> = (0..max_players)
        .filter(|p| !eliminated.contains(p) && (*p == current || has_pieces(board, *p)))
        .collect();
    if alive.len() <= 1 { return current; }
    let idx = alive.iter().position(|&p| p == current).unwrap_or(0);
    alive[(idx + 1) % alive.len()]
}

/// 首步选中心
fn first_move_center(board: &GameBoard, sz: usize) -> Option<(usize, usize)> {
    let mut candidates: Vec<(usize, usize)> = (1..sz - 1)
        .flat_map(|i| (1..sz - 1).filter_map(move |j| {
            if board[i][j].owner.is_none() && !is_in_any_restricted_zone(board, sz, i, j) {
                Some((i, j))
            } else { None }
        })).collect();
    if candidates.is_empty() {
        for i in 0..sz { for j in 0..sz {
            if board[i][j].owner.is_none() && !is_in_any_restricted_zone(board, sz, i, j) {
                candidates.push((i, j));
            }
        }}
    }
    if candidates.is_empty() {
        for i in 0..sz { for j in 0..sz {
            if board[i][j].owner.is_none() { candidates.push((i, j)); }
        }}
    }
    if candidates.is_empty() { return None; }
    let cx = sz as f64 / 2.0 - 0.5;
    candidates.into_iter()
        .min_by(|&(i1, j1), &(i2, j2)| {
            let d1 = (i1 as f64 - cx).abs() + (j1 as f64 - cx).abs();
            let d2 = (i2 as f64 - cx).abs() + (j2 as f64 - cx).abs();
            d1.partial_cmp(&d2).unwrap()
        })
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
fn count_opponent_level_around(board: &GameBoard, sz: usize, i: usize, j: usize, player: usize, level: u8, border_mode: BorderMode) -> i32 {
    let mut cnt = 0;
    for (ni, nj) in nbrs_with_mode(i, j, sz, border_mode) {
        let nc = &board[ni][nj];
        if nc.owner.is_some() && nc.owner != Some(player) && nc.count == level {
            cnt += 1;
        }
    }
    cnt
}

/// 检查 (i,j) 周围是否有指定等级的对手棋子（回环模式邻居按边界模式计算）
fn has_opponent_level_near(board: &GameBoard, sz: usize, i: usize, j: usize, player: usize, level: u8, border_mode: BorderMode) -> bool {
    for (ni, nj) in nbrs_with_mode(i, j, sz, border_mode) {
        let nc = &board[ni][nj];
        if nc.owner.is_some() && nc.owner != Some(player) && nc.count == level {
            return true;
        }
    }
    false
}

/// 统计 (i,j) 周围任意对手棋子数（回环模式邻居按边界模式计算）
fn count_any_opponent_around(board: &GameBoard, sz: usize, i: usize, j: usize, player: usize, border_mode: BorderMode) -> i32 {
    let mut cnt = 0;
    for (ni, nj) in nbrs_with_mode(i, j, sz, border_mode) {
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
    // 收集己方棋子
    let mut mine: Vec<(usize, usize)> = Vec::new();
    for i in 0..sz {
        for j in 0..sz {
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

    // 该格临界等级（再落一子即炸）：cap3→2、cap4→3、cap5→4；degrade 边界格子更低
    let crit_of = |i: usize, j: usize| crit_level(i, j, sz, border_mode, cap_mode) as u8;

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
            let cnt = count_opponent_level_around(board, sz, i, j, player, crit_of(i, j), border_mode);
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
        .filter(|&&(i, j)| !has_opponent_level_near(board, sz, i, j, player, crit_of(i, j), border_mode))
        .copied()
        .collect();
    if !safe_sub.is_empty() {
        let mut best = Vec::new();
        let mut best_score = f64::NEG_INFINITY;
        for &(i, j) in &safe_sub {
            let edge = if i == 0 || i == sz - 1 || j == 0 || j == sz - 1 { 3.0 } else { 0.0 };
            let corner = if (i == 0 || i == sz - 1) && (j == 0 || j == sz - 1) { 5.0 } else { 0.0 };
            let near_any_opp = count_any_opponent_around(board, sz, i, j, player, border_mode);
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
            let near_opp = count_any_opponent_around(board, sz, i, j, player, border_mode);
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
            let near_opp = count_any_opponent_around(board, sz, i, j, player, border_mode);
            let edge = if i == 0 || i == sz - 1 || j == 0 || j == sz - 1 { 2.0 } else { 0.0 };
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
        .filter(|&&(i, j)| (board[i][j].count as u32) < cell_threshold(i, j, sz, border_mode, cap_mode))
        .copied()
        .collect();
    if !available.is_empty() {
        return Some(available[rng.gen_range(0..available.len())]);
    }

    // 6. 保底：优先返回可下棋子；若棋盘存在 count>=阈值 的异常状态（如模拟器构造的
    //    非法初始局面），仍返回第一个己方棋子让 process_click 按爆炸处理，避免无棋可下
    mine.iter().copied()
        .find(|&(i, j)| (board[i][j].count as u32) < cell_threshold(i, j, sz, border_mode, cap_mode))
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

fn default_true() -> bool { true }

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

// ═══════ WASM 导出（JSON 进出） ═══════
#[cfg(target_arch = "wasm32")]
mod wasm_exports {
    use super::*;
    use wasm_bindgen::prelude::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct MoveReq {
        board: GameBoard,
        size: usize,
        x: usize,
        y: usize,
        player: usize,
        max_players: usize,
        border_mode: BorderMode,
        #[serde(default)]
        cap_mode: Option<CapMode>,
        #[serde(default)]
        seed: Option<u64>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AiReq {
        board: GameBoard,
        size: usize,
        player: usize,
        depth: usize,
        eliminated: Vec<usize>,
        max_players: usize,
        border_mode: BorderMode,
        #[serde(default)]
        cap_mode: Option<CapMode>,
        game_count: u32,
        first_move_pos: Option<[usize; 2]>,
        use_ml_eval: Option<bool>,
        algorithm: Option<String>,
        random_scale: Option<u32>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SimReq {
        board: GameBoard,
        size: usize,
        max_players: usize,
        cur_player: usize,
        eliminated: Vec<usize>,
        border_mode: BorderMode,
        #[serde(default)]
        cap_mode: Option<CapMode>,
        first_move_pos: Option<[usize; 2]>,
        game_count: u32,
        ai_configs: std::collections::HashMap<String, serde_json::Value>,
    }

    fn ok<T: Serialize>(v: &T) -> String {
        serde_json::json!({"ok": true, "data": v}).to_string()
    }
    fn err(msg: &str) -> String {
        serde_json::json!({"ok": false, "error": msg}).to_string()
    }

    /// process_move 命令
    #[wasm_bindgen]
    pub fn process_move_cmd(json: &str) -> String {
        let r: MoveReq = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return err(&format!("参数解析失败: {}", e)),
        };
        let mut b = r.board.clone();
        let cap_mode = r.cap_mode.unwrap_or_default();
        let (eliminated, chain_count, killed_by, steps, exploded) =
            process_click_with_snapshots(&mut b, r.size, r.x, r.y, r.player, r.max_players, r.border_mode, cap_mode, r.seed);
        let game_over = eliminated.len() >= r.max_players.saturating_sub(1);
        let winner = if game_over {
            let alive: Vec<usize> = (0..r.max_players)
                .filter(|p| !eliminated.contains(p) && has_pieces(&b, *p))
                .collect();
            alive.first().copied()
        } else {
            None
        };
        ok(&ProcessMoveResult { board: b, eliminated, killed_by, chain_count, game_over, winner, steps, exploded })
    }

    /// AI 走棋命令（按 algorithm 分派：mcts / pvs / alphabeta / strategy）
    #[wasm_bindgen]
    pub fn ai_move_cmd(json: &str) -> String {
        let r: AiReq = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return err(&format!("参数解析失败: {}", e)),
        };
        let use_ml = r.use_ml_eval.unwrap_or(true);
        let alg = r.algorithm.as_deref().unwrap_or("alphabeta");
        let rnd = r.random_scale.unwrap_or(0);
        let cap_mode = r.cap_mode.unwrap_or_default();
        let mv = match alg {
            "mcts" => find_best_move_mcts(&r.board, r.size, r.player, r.depth, &r.eliminated, r.max_players, r.border_mode, cap_mode),
            "pvs" => find_best_move_pvs(&r.board, r.size, r.player, r.depth, &r.eliminated, r.max_players, r.game_count, use_ml, r.border_mode, cap_mode, rnd),
            "strategy" => find_best_move_strategy(&r.board, r.size, r.player, &r.eliminated, r.max_players, r.game_count, r.first_move_pos, r.border_mode, cap_mode),
            _ => find_best_move(&r.board, r.size, r.player, r.depth, &r.eliminated, r.max_players, r.game_count, r.first_move_pos, use_ml, r.border_mode, cap_mode, rnd),
        };
        match mv {
            Some((x, y)) => ok(&[x, y]),
            None => err("No valid move"),
        }
    }

    /// 一键终局命令
    #[wasm_bindgen]
    pub fn simulate_to_end_cmd(json: &str) -> String {
        let r: SimReq = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return err(&format!("参数解析失败: {}", e)),
        };
        let result = simulate_to_end(
            r.board, r.size, r.max_players, r.cur_player, r.eliminated,
            r.border_mode, r.cap_mode.unwrap_or_default(), r.first_move_pos, r.game_count, r.ai_configs,
        );
        ok(&result)
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BenchPlayer {
        algorithm: String,
        #[serde(default)]
        depth: Option<usize>,
        #[serde(default)]
        random_scale: Option<u32>,
        #[serde(default)]
        use_ml_eval: Option<bool>,
    }

    fn default_bench_border() -> BorderMode { BorderMode::Default }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BenchReq {
        size: usize,
        players: Vec<BenchPlayer>,
        #[serde(default)]
        game_count: u32,
        #[serde(default = "default_bench_border")]
        border_mode: BorderMode,
        #[serde(default)]
        cap_mode: CapMode,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct BenchResult {
        elapsed_ms: f64,
        steps: usize,
        winner: Option<usize>,
    }

    /// 单局 AI 基准命令（与 Tauri 端 bench_ai_game 语义一致，供设备性能检测页使用）
    #[wasm_bindgen]
    pub fn bench_ai_game_cmd(json: &str) -> String {
        let r: BenchReq = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return err(&format!("参数解析失败: {}", e)),
        };
        let sz = r.size;
        let max_players = r.players.len();
        if sz < 5 || max_players < 2 {
            return ok(&BenchResult { elapsed_ms: 0.0, steps: 0, winner: None });
        }
        let mut board: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None }; sz]; sz];
        let starts = spread_starts(sz, max_players);
        // 首子等级 = 阈值 n-1（cap3→2、cap4→3、cap5→4；随机模式取中间等级 3）
        let th: u32 = match r.cap_mode {
            CapMode::Cap3 => 3,
            CapMode::Cap4 => 4,
            CapMode::Cap5 => 5,
            CapMode::Random => 3,
        };
        for (p, &(x, y)) in starts.iter().enumerate() {
            board[x][y] = Cell { owner: Some(p), count: (th - 1) as u8, th: None };
        }
        let mut ai_configs: HashMap<String, serde_json::Value> = HashMap::new();
        for (p, pl) in r.players.iter().enumerate() {
            let is_mcts = pl.algorithm == "mcts";
            ai_configs.insert(
                p.to_string(),
                serde_json::json!({
                    "algorithm": pl.algorithm,
                    "depth": pl.depth.unwrap_or(if is_mcts { 1 } else { 2 }),
                    "useMlEval": pl.use_ml_eval.unwrap_or(true),
                    "randomScale": pl.random_scale.unwrap_or(0),
                }),
            );
        }
        // 计时由 engine.js 用 performance.now() 完成（wasm32-unknown-unknown 无标准时钟）
        let res = simulate_to_end(board, sz, max_players, 0, Vec::new(), r.border_mode, r.cap_mode, None, r.game_count, ai_configs);
        ok(&BenchResult { elapsed_ms: 0.0, steps: res.history.len(), winner: res.winner })
    }

    #[wasm_bindgen]
    pub fn engine_version() -> String {
        "chain-chess-engine-wasm-3.2.3".to_string()
    }
}

// ═══════════════════ 规则完备性测试（borderMode × capMode 全组合） ═══════════════════
#[cfg(test)]
mod tests {
    use super::*;

    const ALL_BM: [BorderMode; 5] = [BorderMode::Default, BorderMode::Wrap, BorderMode::Bounce, BorderMode::Degrade, BorderMode::Random];
    const ALL_CM: [CapMode; 4] = [CapMode::Cap3, CapMode::Cap4, CapMode::Cap5, CapMode::Random];

    fn mk_b(sz: usize) -> GameBoard {
        vec![vec![Cell { owner: None, count: 0, th: None }; sz]; sz]
    }
    fn set(b: &mut GameBoard, x: usize, y: usize, owner: usize, count: u8) {
        b[x][y] = Cell { owner: Some(owner), count, th: None };
    }
    fn do_click(b: &GameBoard, sz: usize, x: usize, y: usize, pl: usize, max: usize, bm: BorderMode, cm: CapMode, seed: Option<u64>) -> (GameBoard, Vec<usize>, Vec<(usize, usize)>) {
        let mut nb = b.clone();
        let (elim, _cc, kb) = process_click_with_killer(&mut nb, sz, x, y, pl, max, bm, cm, seed);
        (nb, elim, kb)
    }
    /// 某个格子在孤立场景（邻居全空）下，落子后是否触发爆炸
    fn boom_check(bm: BorderMode, cm: CapMode, start_count: u8, seed: u64) -> (bool, GameBoard) {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, start_count);
        let (nb, _, _) = do_click(&b, sz, 2, 2, 0, 2, bm, cm, Some(seed));
        (nb[2][2].owner.is_none(), nb)
    }

    // ── 1) 阈值正确性：所有 borderMode × capMode 组合 ──
    #[test]
    fn threshold_cap3_all_borders() {
        for &bm in &ALL_BM {
            // 2 子 +1 → 3 炸（cap3 阈值）
            let (boom, _) = boom_check(bm, CapMode::Cap3, 2, 1);
            assert!(boom, "cap3 {bm:?}: count2+1 should boom");
            // 1 子 +1 → 2 不炸
            let (boom2, nb) = boom_check(bm, CapMode::Cap3, 1, 1);
            assert!(!boom2, "cap3 {bm:?}: count1+1 should not boom");
            assert_eq!(nb[2][2].count, 2);
        }
    }
    #[test]
    fn threshold_cap4_all_borders() {
        for &bm in &ALL_BM {
            // 3 子 +1 → 4 炸；degrade 中央阈值仍 4
            let (boom, _) = boom_check(bm, CapMode::Cap4, 3, 1);
            assert!(boom, "cap4 {bm:?}: count3+1 should boom");
            let (boom2, nb) = boom_check(bm, CapMode::Cap4, 2, 1);
            assert!(!boom2, "cap4 {bm:?}: count2+1 should not boom");
            assert_eq!(nb[2][2].count, 3);
        }
        // degrade：角上 1 子 +1 → 2 炸（位置修正）
        let mut b = mk_b(5);
        set(&mut b, 0, 0, 0, 1);
        let (nb, _, _) = do_click(&b, 5, 0, 0, 0, 2, BorderMode::Degrade, CapMode::Cap4, Some(1));
        assert!(nb[0][0].owner.is_none(), "degrade corner count1+1 should boom");
    }
    #[test]
    fn threshold_cap5_all_borders() {
        for &bm in &ALL_BM {
            // 4 子 +1 → 5 炸
            let (boom, _) = boom_check(bm, CapMode::Cap5, 4, 1);
            assert!(boom, "cap5 {bm:?}: count4+1 should boom");
            let (boom2, nb) = boom_check(bm, CapMode::Cap5, 3, 1);
            assert!(!boom2, "cap5 {bm:?}: count3+1 should not boom");
            assert_eq!(nb[2][2].count, 4);
        }
    }
    /// capMode 优先于 degrade 位置修正（cap3 在 degrade 角落仍 3 级炸）
    #[test]
    fn capmode_overrides_degrade() {
        let mut b = mk_b(5);
        set(&mut b, 0, 0, 0, 2);
        let (nb, _, _) = do_click(&b, 5, 0, 0, 0, 2, BorderMode::Degrade, CapMode::Cap3, Some(1));
        assert!(nb[0][0].owner.is_none(), "cap3+degrade corner count2+1 should boom");
        let mut b5 = mk_b(5);
        set(&mut b5, 0, 0, 0, 1);
        let (nb5, _, _) = do_click(&b5, 5, 0, 0, 0, 2, BorderMode::Degrade, CapMode::Cap5, Some(1));
        assert_eq!(nb5[0][0].count, 2, "cap5+degrade corner count1+1 should NOT boom");
    }

    // ── 2) 边界模式扩散正确性 ──
    #[test]
    fn diffusion_default_wrap_bounce() {
        // default：角落爆炸向 2 个邻居扩散
        let mut b = mk_b(5);
        set(&mut b, 0, 0, 0, 3);
        let (nb, _, _) = do_click(&b, 5, 0, 0, 0, 2, BorderMode::Default, CapMode::Cap4, Some(1));
        assert_eq!(nb[1][0].count, 1);
        assert_eq!(nb[0][1].count, 1);
        assert!(nb[0][0].owner.is_none());
        // wrap：角落爆炸回环到对面
        let mut bw = mk_b(5);
        set(&mut bw, 0, 0, 0, 3);
        let (nbw, _, _) = do_click(&bw, 5, 0, 0, 0, 2, BorderMode::Wrap, CapMode::Cap4, Some(1));
        assert_eq!(nbw[1][0].count, 1);
        assert_eq!(nbw[0][1].count, 1);
        assert_eq!(nbw[4][0].count, 1, "wrap corner should wrap to bottom");
        assert_eq!(nbw[0][4].count, 1, "wrap corner should wrap to right");
        // bounce：角落爆炸，有效方向 (1,0),(0,1) 各 +1（基础），上/左出界能量反弹到正对方向再 +1
        let mut bb = mk_b(5);
        set(&mut bb, 0, 0, 0, 3);
        let (nbb, _, _) = do_click(&bb, 5, 0, 0, 0, 2, BorderMode::Bounce, CapMode::Cap4, Some(1));
        assert_eq!(nbb[1][0].count, 2, "bounce corner: (1,0) gets base+rebound");
        assert_eq!(nbb[0][1].count, 2, "bounce corner: (0,1) gets base+rebound");
        assert!(nbb[0][0].owner.is_none());
        // 中央爆炸无出界：4 方向各 +1（无反弹）
        let mut bc = mk_b(5);
        set(&mut bc, 2, 2, 0, 3);
        let (nbc, _, _) = do_click(&bc, 5, 2, 2, 0, 2, BorderMode::Bounce, CapMode::Cap4, Some(1));
        for (x, y) in [(1usize, 2usize), (3, 2), (2, 1), (2, 3)] {
            assert_eq!(nbc[x][y].count, 1, "bounce center: no rebound");
        }
    }

    // ── 3) 随机行为：cap3 随机一边加 0 / cap5 随机一边加 2，全边界模式 ──
    #[test]
    fn cap3_random_zero_add_all_borders() {
        for &bm in &ALL_BM {
            let sz = 5;
            let mut b = mk_b(sz);
            set(&mut b, 2, 2, 0, 2);
            let (nb, _, _) = do_click(&b, sz, 2, 2, 0, 2, bm, CapMode::Cap3, Some(42));
            let dirs = [(1usize, 2usize), (3, 2), (2, 1), (2, 3)];
            let own0 = dirs.iter().filter(|(x, y)| nb[*x][*y].owner == Some(0)).count();
            let empty = dirs.iter().filter(|(x, y)| nb[*x][*y].owner.is_none()).count();
            assert_eq!(own0, 3, "cap3 {bm:?}: exactly 3 dirs get piece");
            assert_eq!(empty, 1, "cap3 {bm:?}: exactly 1 dir cleared");
            assert!(nb[2][2].owner.is_none());
        }
    }
    #[test]
    fn cap5_random_plus_two_all_borders() {
        for &bm in &ALL_BM {
            let sz = 5;
            let mut b = mk_b(sz);
            set(&mut b, 2, 2, 0, 4);
            let (nb, _, _) = do_click(&b, sz, 2, 2, 0, 2, bm, CapMode::Cap5, Some(77));
            let dirs = [(1usize, 2usize), (3, 2), (2, 1), (2, 3)];
            let two = dirs.iter().filter(|(x, y)| nb[*x][*y].count == 2).count();
            let one = dirs.iter().filter(|(x, y)| nb[*x][*y].count == 1).count();
            assert_eq!(two, 1, "cap5 {bm:?}: exactly 1 dir becomes level 2");
            assert_eq!(one, 3, "cap5 {bm:?}: other 3 dirs +1");
        }
    }
    #[test]
    fn randomness_deterministic_by_seed() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 2);
        let (nb1, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap3, Some(42));
        let (nb2, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap3, Some(42));
        assert_eq!(nb1, nb2, "same seed must give same result");
        // 遍历多个 seed，应出现不同的清空格位置
        let mut cleared_positions = std::collections::HashSet::new();
        for seed in 0..12u64 {
            let (nb, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap3, Some(seed));
            for (x, y) in [(1usize, 2usize), (3, 2), (2, 1), (2, 3)] {
                if nb[x][y].owner.is_none() { cleared_positions.insert((x, y)); }
            }
        }
        assert!(cleared_positions.len() > 1, "different seeds should vary cleared cell");
    }

    // ── 3b) 加 0 / 加 2 作用于"有棋子"的格子（不清空、不强制等级） ──
    #[test]
    fn cap3_zero_keeps_owned_cell() {
        // 上邻居是玩家1 的 1 级格：cap3 特殊格若选中它，应"加 0"（保持 1:1），不是清空
        let sz = 5;
        for seed in 0..40u64 {
            let mut b = mk_b(sz);
            set(&mut b, 2, 2, 0, 2);          // 落子 → 3 → 爆炸
            set(&mut b, 1, 2, 1, 1);          // 上邻居：玩家1 1 级
            let (nb, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap3, Some(seed));
            let up = nb[1][2];
            // 爆炸后上邻居只可能有两种结果：
            //  - 特殊格加 0：保持玩家1 count1（原样）
            //  - 普通加 1：变成玩家0 count2（覆盖+1）
            assert!(
                (up.owner == Some(1) && up.count == 1) || (up.owner == Some(0) && up.count == 2),
                "cap3 上邻居不应被清空: got {:?}", up
            );
        }
    }
    #[test]
    fn cap5_two_adds_on_owned_cell() {
        // 上邻居是玩家1 的 1 级格：cap5 特殊格若选中它，应"加 2"变 3 级（0:3），不是强制 2 级
        let sz = 5;
        let mut hit_plus2 = false;
        for seed in 0..60u64 {
            let mut b = mk_b(sz);
            set(&mut b, 2, 2, 0, 4);          // 落子 → 5 → 爆炸
            set(&mut b, 1, 2, 1, 1);          // 上邻居：玩家1 1 级
            let (nb, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap5, Some(seed));
            let up = nb[1][2];
            //  - 特殊格加 2：玩家0 count3（1+2，覆盖）
            //  - 普通加 1：玩家0 count2
            if up.owner == Some(0) && up.count == 3 {
                hit_plus2 = true;
            }
            assert!(
                up.owner == Some(0) && (up.count == 2 || up.count == 3),
                "cap5 上邻居应为覆盖+1或+2: got {:?}", up
            );
        }
        assert!(hit_plus2, "cap5 特殊格应至少一次加 2（1 级格 → 3 级）");
    }

    // ── 4) 淘汰与击败者 ──
    #[test]
    fn elimination_and_killer() {
        let sz = 5;
        let mut b = mk_b(sz);
        // 玩家1 只有 1 颗子，被玩家0 爆炸覆盖 → 淘汰
        set(&mut b, 2, 2, 0, 3);
        set(&mut b, 2, 3, 1, 1);
        let (nb, elim, kb) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap4, Some(1));
        assert_eq!(nb[2][2].owner, None);
        assert_eq!(elim, vec![1], "player1 should be eliminated");
        assert!(kb.contains(&(1, 0)), "killer of player1 should be player0: {:?}", kb);
    }

    // ── 5) 死锁防御：低阈值相邻互供能棋盘不卡死 ──
    #[test]
    fn no_deadlock_low_threshold() {
        let sz = 5;
        let mut b = mk_b(sz);
        // 3×3 区域全部 2 子同玩家：cap3 下任何落子都引爆大规模连锁
        for i in 1..4 { for j in 1..4 { set(&mut b, i, j, 0, 2); } }
        let (nb, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap3, Some(1));
        // 跑完不 panic 即通过（MAX_CHAIN_STEPS 防御生效）
        let _ = nb;
    }

    // ── 7) simulate_to_end 全组合可跑通 + mv 重放一致 ──
    #[test]
    fn simulate_to_end_all_combos_replayable() {
        for &bm in &ALL_BM {
            for &cm in &ALL_CM {
                let sz = 7;
                let max_players = 3;
                let mut b = mk_b(sz);
                for (x, y, p) in [(0usize, 0usize, 0usize), (0, 6, 1), (6, 0, 2), (6, 6, 0), (3, 3, 1), (1, 1, 2)] {
                    b[x][y] = Cell { owner: Some(p), count: 3, th: b[x][y].th };
                }
                let mut cfg = std::collections::HashMap::new();
                for p in 0..max_players {
                    cfg.insert(p.to_string(), serde_json::json!({"algorithm": "strategy", "depth": 1, "useMlEval": true}));
                }
                let r = simulate_to_end(b.clone(), sz, max_players, 0, vec![], bm, cm, None, 0, cfg);
                assert!(!r.history.is_empty(), "{bm:?}×{cm:?}: sim should produce history");
                assert!(r.winner.is_some() || !r.eliminated_order.is_empty(), "{bm:?}×{cm:?}: sim should conclude");
                // mv 重放一致性
                let mut rb = b.clone();
                for h in &r.history {
                    let mv = h.mv.expect("mv with seed");
                    let (x, y, pl, seed) = (mv[0] as usize, mv[1] as usize, mv[2] as usize, mv[3]);
                    process_click_with_killer(&mut rb, sz, x, y, pl, max_players, bm, cm, Some(seed));
                }
                assert_eq!(rb, r.board, "{bm:?}×{cm:?}: replay must match simulation");
            }
        }
    }

    // ── 8) 首子等级 = 阈值 n-1（临界态） ──
    #[test]
    fn first_move_level_n_minus_1() {
        let sz = 5;
        // cap3：首子 2 级
        let b = mk_b(sz);
        let (nb, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap3, Some(1));
        assert_eq!(nb[2][2].count, 2, "cap3 首子应为 2 级");
        assert_eq!(nb[2][2].owner, Some(0));
        // cap4：首子 3 级
        let b4 = mk_b(sz);
        let (nb4, _, _) = do_click(&b4, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap4, Some(1));
        assert_eq!(nb4[2][2].count, 3, "cap4 首子应为 3 级");
        // cap5：首子 4 级
        let b5 = mk_b(sz);
        let (nb5, _, _) = do_click(&b5, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap5, Some(1));
        assert_eq!(nb5[2][2].count, 4, "cap5 首子应为 4 级");
        // 首子 n-1 不触发爆炸
        assert!(nb[2][2].owner.is_some(), "cap3 首子 2 级不应爆炸");
    }

    // ── 9) 随机阈值模式（capMode=random）：每步随机 3/4/5，seed 确定可复现 ──
    #[test]
    fn capmode_random_thresholds() {
        let sz = 5;
        // 3 子 +1：阈值 3 时炸；阈值 4/5 时不炸
        let mut th3 = false; let mut th5 = false;
        for seed in 0..60u64 {
            let mut b3 = mk_b(sz); set(&mut b3, 2, 2, 0, 2);
            let (n3, _, _) = do_click(&b3, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Random, Some(seed));
            if n3[2][2].owner.is_none() { th3 = true; } // 阈值=3：2+1=3 炸
            let mut b5 = mk_b(sz); set(&mut b5, 2, 2, 0, 3);
            let (n5, _, _) = do_click(&b5, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Random, Some(seed));
            if n5[2][2].owner.is_some() && n5[2][2].count == 4 { th5 = true; } // 阈值=5：3+1=4 不炸
        }
        assert!(th3, "随机阈值应出现 3");
        assert!(th5, "随机阈值应出现 5");
        // 同 seed 确定性
        let mut b = mk_b(sz); set(&mut b, 2, 2, 0, 2);
        let (r1, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Random, Some(7));
        let (r2, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Random, Some(7));
        assert_eq!(r1, r2, "同 seed 随机阈值应一致");
    }

    // ── 10) 随机边界模式（borderMode=random）：每步随机边界行为，seed 确定可复现 ──
    #[test]
    fn bordermode_random_runs() {
        let sz = 5;
        // 角落爆炸：不同边界扩散目标不同（default 2 方向 vs wrap 回环 4 方向等）
        let mut b = mk_b(sz);
        set(&mut b, 0, 0, 0, 3);
        let (nb, _, _) = do_click(&b, sz, 0, 0, 0, 2, BorderMode::Random, CapMode::Cap4, Some(5));
        assert!(nb[0][0].owner.is_none(), "随机边界下 cap4 3+1=4 应炸");
        // 同 seed 确定性
        let (r1, _, _) = do_click(&b, sz, 0, 0, 0, 2, BorderMode::Random, CapMode::Cap4, Some(5));
        let (r2, _, _) = do_click(&b, sz, 0, 0, 0, 2, BorderMode::Random, CapMode::Cap4, Some(5));
        assert_eq!(r1, r2, "同 seed 随机边界应一致");
        // 不同 seed 出现不同扩散（回环/反弹等不同目标集合）
        let mut diff = false;
        let base = do_click(&b, sz, 0, 0, 0, 2, BorderMode::Random, CapMode::Cap4, Some(1)).0;
        for seed in 0..20u64 {
            let r = do_click(&b, sz, 0, 0, 0, 2, BorderMode::Random, CapMode::Cap4, Some(seed)).0;
            if r != base { diff = true; break; }
        }
        assert!(diff, "随机边界不同 seed 应有不同扩散结果");
    }

    // ── 11) 阈值感知走法生成：cap5 下 count==4 的引爆动作必须合法 ──
    #[test]
    fn get_moves_cap5_includes_explosive() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4); // 己方临界 count4（再落一子即炸）
        let mvs5 = get_moves(&b, sz, 0, None, BorderMode::Default, CapMode::Cap5);
        assert!(mvs5.contains(&(2, 2)), "cap5: count4 explosive move must be legal, got {:?}", mvs5);
        // cap3 下 count4 不可能存在，也不应被允许
        let mvs3 = get_moves(&b, sz, 0, None, BorderMode::Default, CapMode::Cap3);
        assert!(!mvs3.contains(&(2, 2)), "cap3: count4 must not be legal");
        // cap3 下临界 count2 的引爆动作合法（count < 3）
        let mut b3 = mk_b(sz);
        set(&mut b3, 2, 2, 0, 2);
        let mvs3b = get_moves(&b3, sz, 0, None, BorderMode::Default, CapMode::Cap3);
        assert!(mvs3b.contains(&(2, 2)), "cap3: count2 critical move must be legal");
    }

    // ── 12) 策略 AI：cap3 模式“二二相接”引爆、cap5 模式“四四相接”引爆 ──
    #[test]
    fn strategy_cap3_explodes_on_pair() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 2); // 己方临界
        set(&mut b, 2, 3, 1, 2); // 对手临界相邻
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Default, CapMode::Cap3);
        assert_eq!(mv, Some((2, 2)), "cap3 strategy should explode pair (2v2), got {:?}", mv);
    }
    #[test]
    fn strategy_cap5_explodes_on_quad() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4); // 己方临界
        set(&mut b, 2, 3, 1, 4); // 对手临界相邻
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Default, CapMode::Cap5);
        assert_eq!(mv, Some((2, 2)), "cap5 strategy should explode quad (4v4), got {:?}", mv);
    }
    #[test]
    fn strategy_cap4_explodes_on_triple() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 3);
        set(&mut b, 2, 3, 1, 3);
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Default, CapMode::Cap4);
        assert_eq!(mv, Some((2, 2)), "cap4 strategy should explode triple (3v3), got {:?}", mv);
    }

    // ── 13) 策略 AI：cap3 下“建立临界”升 count1→2，不选 count2→3 的自爆走法 ──
    #[test]
    fn strategy_cap3_no_suicide() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 1); // 己方 count1（升到 2 安全）
        set(&mut b, 1, 1, 0, 2); // 己方 count2（cap3 下再落子自爆）
        set(&mut b, 4, 4, 1, 1); // 远处对手
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Default, CapMode::Cap3);
        assert_eq!(mv, Some((2, 2)), "cap3 strategy must upgrade 1->2, not suicide 2->3, got {:?}", mv);
    }

    // ── 14) 回环模式：策略 AI 的上下左右判断与回环相符（最上行上方=最下行） ──
    #[test]
    fn strategy_wrap_neighbors_loop() {
        let sz = 5;
        let mut b = mk_b(sz);
        // 己方 (0,2) count1（cap3 下安全升级位）；对手临界 count2 在 (4,2)（wrap 中位于其正上方）
        set(&mut b, 0, 2, 0, 1);
        set(&mut b, 4, 2, 1, 2);
        // wrap：对手临界贴着己方安全位 → 步骤2 排除（不安全），应走建立临界（也是 (0,2)）或引爆
        let mv_wrap = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Wrap, CapMode::Cap3);
        // 不允许升级到会引爆的位置？(0,2) 1->2 安全，仍是最优（建立临界靠近对手）
        assert_eq!(mv_wrap, Some((0, 2)), "wrap: expected (0,2), got {:?}", mv_wrap);
        // default 模式下 (4,2) 不在 (0,2) 邻域 → 无对手临近 → 仍是 (0,2)
        let mv_def = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Default, CapMode::Cap3);
        assert_eq!(mv_def, Some((0, 2)), "default: expected (0,2), got {:?}", mv_def);
    }
    #[test]
    fn strategy_wrap_detects_wrapped_threat() {
        let sz = 5;
        let mut b = mk_b(sz);
        // 己方临界 count2 在 (0,2)，对手临界 count2 在 (4,2)：wrap 下二者相邻 → 应引爆
        set(&mut b, 0, 2, 0, 2);
        set(&mut b, 4, 2, 1, 2);
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Wrap, CapMode::Cap3);
        assert_eq!(mv, Some((0, 2)), "wrap: wrapped adjacency should trigger explosion, got {:?}", mv);
    }

    // ── 15) MCTS 在 cap5 下能够执行引爆走法（count==4 合法） ──
    #[test]
    fn mcts_cap5_can_explode() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4);
        set(&mut b, 2, 3, 1, 4);
        let mv = find_best_move_mcts(&b, sz, 0, 2, &[], 2, BorderMode::Default, CapMode::Cap5);
        assert_eq!(mv, Some((2, 2)), "cap5 MCTS should pick explosive move, got {:?}", mv);
    }

    // ── 16) PVS 在 cap5 下能够执行引爆走法 ──
    #[test]
    fn pvs_cap5_can_explode() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4);
        set(&mut b, 2, 3, 1, 4);
        let mv = find_best_move_pvs(&b, sz, 0, 2, &[], 2, 0, true, BorderMode::Default, CapMode::Cap5, 0);
        assert_eq!(mv, Some((2, 2)), "cap5 PVS should pick explosive move, got {:?}", mv);
    }

    // ── 17) Alpha-Beta 在 cap5 下能够执行引爆走法 ──
    // ── MCTS 性能基准（手动运行：cargo test --release mcts_bench -- --ignored --nocapture） ──
    #[test]
    #[ignore]
    fn mcts_bench() {
        use std::time::Instant;
        let sz = 7;
        for cm in [CapMode::Cap3, CapMode::Cap4, CapMode::Cap5] {
            let mut b = mk_b(sz);
            for i in 1..6 { for j in 1..6 {
                if (i + j) % 2 == 0 { set(&mut b, i, j, (i % 2) as usize, 2); }
            }}
            for depth in [1usize, 2, 3] {
                let t0 = Instant::now();
                let mv = find_best_move_mcts(&b, sz, 0, depth, &[], 2, BorderMode::Default, cm);
                let dt = t0.elapsed();
                println!("MCTS {:?} depth={} → {:?}  耗时 {:?}", cm, depth, mv, dt);
            }
        }
    }

    // ── 18) 新 18 维训练模型与 Rust 端加载兼容（冒烟测试产物存在时验证） ──
    #[test]
    fn xgb_v2_model_loadable() {
        let path = "/tmp/xgb_smoke.json";
        if std::path::Path::new(path).exists() {
            let eng = get_xgb_engine_for_test(path);
            let feats = [0.0f32; FEAT_DIM];
            let (raw, prob) = eng.predict(&feats);
            assert!(raw.is_finite(), "raw={raw}");
            assert!(prob.is_finite() && prob > 0.0 && prob < 1.0, "prob={prob}");
        }
    }

    #[test]
    fn alphabeta_cap5_can_explode() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4);
        set(&mut b, 2, 3, 1, 4);
        let mv = find_best_move(&b, sz, 0, 2, &[], 2, 0, None, true, BorderMode::Default, CapMode::Cap5, 0);
        assert_eq!(mv, Some((2, 2)), "cap5 alphabeta should pick explosive move, got {:?}", mv);
    }

    // ── 设备性能检测：首子布局合法性 ──
    #[test]
    fn bench_spread_starts_valid() {
        for &(sz, n) in &[(7usize, 4usize), (9, 6), (11, 6), (5, 2)] {
            let pos = spread_starts(sz, n);
            assert_eq!(pos.len(), n, "{sz}×{sz} {n} 人首子数量");
            for &(x, y) in &pos {
                assert!(x < sz && y < sz, "首子在界内");
            }
            for i in 0..n {
                for j in (i + 1)..n {
                    let d = pos[i].0.abs_diff(pos[j].0).max(pos[i].1.abs_diff(pos[j].1));
                    assert!(d >= 3, "{sz}×{sz}: 首子 ({:?}) 与 ({:?}) 距离 {d} < 3", pos[i], pos[j]);
                }
            }
        }
    }
}
