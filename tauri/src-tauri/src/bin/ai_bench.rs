// 连锁棋 AI 实力基准（供 ai_strength.py 调度）
//
// 从 stdin 读取一个 JSON 配置，逐局用 tauri 端引擎 simulate_to_end 模拟 AI 对局，
// stdout 每行输出一个 JSON 结果（行协议），Python 端实时读取并展示进度条。
//
// 配置格式（JSON）:
// {
//   "size": 7, "games": 6, "mode": "ffa" | "duel",
//   "shapes": ["square","rect","holes","ring","cross"],   // 棋盘形状（缺省 square）
//   "borders": ["default","wrap","bounce","degrade"],
//   "caps": ["3","4","5"],
//   "algs": ["strategy","alphabeta","pvs","mcts"],   // ffa 模式每玩家一算法
//   "duelPairs": [["strategy","alphabeta"]],          // duel 模式（缺省自动全配对）
//   "depth": 2, "mctsDepth": 1, "randomScale": 5, "useMlEval": true,
//   "seed": 0
// }
//
// 输出（每行一个）:
// {"id":0,"shape":"square","bm":"default","cm":"4","mode":"ffa","players":[...],"winner":0,"steps":123,"order":[2,3,1]}
//
// 用法: cargo run --release --bin ai_bench < config.json
use std::io::{self, Read, Write};
use serde::Deserialize;
use chain_chess_core::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BenchConfig {
    #[serde(default = "default_size")]
    size: usize,
    #[serde(default = "default_games")]
    games: u32,
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default = "default_shapes")]
    shapes: Vec<String>,
    #[serde(default = "default_borders")]
    borders: Vec<BorderMode>,
    #[serde(default = "default_caps")]
    caps: Vec<CapMode>,
    #[serde(default = "default_algs")]
    algs: Vec<String>,
    #[serde(default)]
    duel_pairs: Vec<Vec<String>>,
    #[serde(default = "default_depth")]
    depth: usize,
    #[serde(default = "default_mcts_depth")]
    mcts_depth: usize,
    #[serde(default = "default_random")]
    random_scale: u32,
    #[serde(default = "default_ml")]
    use_ml_eval: bool,
    #[serde(default)]
    seed: u32,
}
fn default_size() -> usize { 7 }
fn default_games() -> u32 { 6 }
fn default_mode() -> String { "ffa".to_string() }
fn default_shapes() -> Vec<String> { vec!["square".to_string()] }
fn default_borders() -> Vec<BorderMode> {
    vec![BorderMode::Default, BorderMode::Wrap, BorderMode::Bounce, BorderMode::Degrade]
}
fn default_caps() -> Vec<CapMode> {
    vec![CapMode::Cap3, CapMode::Cap4, CapMode::Cap5]
}
fn default_algs() -> Vec<String> {
    vec!["strategy".into(), "alphabeta".into(), "pvs".into(), "mcts".into()]
}
fn default_depth() -> usize { 2 }
fn default_mcts_depth() -> usize { 1 }
fn default_random() -> u32 { 5 }
fn default_ml() -> bool { true }

/// 棋盘形状 → (行, 列, 可用位图)。全部确定性生成（不用随机），
/// 这样同一 seed 下的成绩可以跨版本直接比。
///
/// 形状与前端形状编辑器的预设保持一致（`applyBePreset`，边宽取 1/3）：
/// - square：正方形，全部可用（与旧口径一致，可对比历史数据）
/// - rect：长方形，列比行多 2
/// - holes：正方形，中央挖两个对角洞（四个方向都能碰到洞）
/// - ring：口字，外框宽 `m`，内部掏空
/// - cross：十字，中间横竖各一条宽 `m` 的带子
///
/// 带宽不能取 1：一条线宽的十字在速爆 3 下，爆炸只剩一个可落点，
/// 「随机一个方向不送点」必然命中，玩家会直接自爆，评测数据没有意义。
fn build_shape(kind: &str, size: usize) -> (usize, usize, Vec<Vec<bool>>) {
    let (rows, cols) = match kind {
        "rect" => (size, (size + 2).min(19)),
        _ => (size, size),
    };
    let mut open = vec![vec![true; cols]; rows];
    let m = (rows.min(cols) / 3).max(1);   // 与前端预设一致：边宽取 1/3
    match kind {
        "holes" if rows >= 3 && cols >= 3 => {
            let (a, b) = (rows / 2 - 1, cols / 2 - 1);
            open[a][b] = false;
            open[a + 1][b + 1] = false;
        }
        "ring" => {
            for (i, row) in open.iter_mut().enumerate() {
                for (j, cell) in row.iter_mut().enumerate() {
                    if i >= m && i < rows.saturating_sub(m)
                        && j >= m && j < cols.saturating_sub(m) {
                        *cell = false;
                    }
                }
            }
        }
        "cross" => {
            for (i, row) in open.iter_mut().enumerate() {
                for (j, cell) in row.iter_mut().enumerate() {
                    let in_h = i >= m && i < rows.saturating_sub(m);
                    let in_v = j >= m && j < cols.saturating_sub(m);
                    if !in_h && !in_v { *cell = false; }
                }
            }
        }
        _ => {}   // square / rect：整盘可用
    }
    (rows, cols, open)
}

/// 把形状转成开局棋盘。
fn build_board(rows: usize, cols: usize, open: &[Vec<bool>]) -> GameBoard {
    let mut b = vec![vec![Cell { owner: None, count: 0, th: None, blocked: false }; cols]; rows];
    for (i, row) in open.iter().enumerate() {
        for (j, &ok) in row.iter().enumerate() {
            if !ok { b[i][j].blocked = true; }
        }
    }
    b
}

/// 两个可用格在「当前边界语义」下的最短步数（BFS）。
///
/// 不能只看直线距离：回环会把隔着洞的两格连成邻居 —— 口字棋盘上下两条外框
/// 中间只隔着一排洞，直线看着很远，实际一步就到，首子放这儿会一步被炸光。
fn border_dist(board: &GameBoard, from: (usize, usize), to: (usize, usize), bm: BorderMode) -> usize {
    if from == to { return 0; }
    let rows = board.len();
    let cols = board.first().map_or(0, |r| r.len());
    let mut seen = vec![vec![false; cols]; rows];
    let mut q: std::collections::VecDeque<((usize, usize), usize)> = std::collections::VecDeque::new();
    seen[from.0][from.1] = true;
    q.push_back((from, 0));
    while let Some(((x, y), d)) = q.pop_front() {
        for (nx, ny) in nbrs_with_mode(board, x, y, bm) {
            if seen[nx][ny] { continue; }
            if (nx, ny) == to { return d + 1; }
            seen[nx][ny] = true;
            q.push_back(((nx, ny), d + 1));
        }
    }
    usize::MAX / 4   // 不连通：当作极远
}

/// 合法首子布局：从可用格里挑 n 个，先取贴近中心的，之后每次挑
/// 「按边界语义离已选格子最远」的那个，让开局有展开空间。
///
/// 距离用 BFS（见 border_dist），而不是直线距离 —— 异形棋盘 + 回环下，
/// 直线距离会骗人，导致首子实际紧挨着、一步定胜负。
fn spread_starts(board: &GameBoard, n: usize, bm: BorderMode) -> Vec<(usize, usize)> {
    let rows = board.len();
    let cols = board.first().map_or(0, |r| r.len());
    let mut cand: Vec<(usize, usize)> = Vec::new();
    for i in 0..rows {
        for j in 0..cols {
            if !board[i][j].blocked { cand.push((i, j)); }
        }
    }
    if cand.is_empty() { return cand; }
    let ci = (rows as f64 - 1.0) / 2.0;
    let cj = (cols as f64 - 1.0) / 2.0;
    cand.sort_by(|&(ai, aj), &(bi, bj)| {
        let da = (ai as f64 - ci).abs() + (aj as f64 - cj).abs();
        let db = (bi as f64 - ci).abs() + (bj as f64 - cj).abs();
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut picked: Vec<(usize, usize)> = Vec::with_capacity(n);
    if let Some(&first) = cand.first() { picked.push(first); }
    while picked.len() < n && picked.len() < cand.len() {
        let mut best: Option<(usize, usize)> = None;
        let mut best_d: usize = 0;
        for &p in &cand {
            if picked.contains(&p) { continue; }
            let d = picked.iter().map(|&q| border_dist(board, p, q, bm)).min().unwrap_or(0);
            if best.is_none() || d > best_d { best_d = d; best = Some(p); }
        }
        match best {
            Some(p) => picked.push(p),
            None => break,
        }
    }
    for &p in &cand {
        if picked.len() >= n { break; }
        if !picked.contains(&p) { picked.push(p); }
    }
    picked
}


/// 各算法的搜索深度：MCTS 用自己的深度（迭代 = 深度 × 800）。
fn alg_depth(alg: &str, cfg: &BenchConfig) -> usize {
    if alg == "mcts" { cfg.mcts_depth } else { cfg.depth }
}

/// 跑一局，返回 (winner, steps, eliminated_order)
fn run_game(
    shape: &str,
    size: usize,
    bm: BorderMode,
    cm: CapMode,
    gc: u32,
    players: &[String],
    cfg: &BenchConfig,
) -> (Option<usize>, usize, Vec<usize>) {
    let (rows, cols, open) = build_shape(shape, size);
    let mut board = build_board(rows, cols, &open);
    let starts = spread_starts(&board, players.len(), bm);
    let max_players = players.len();
    assert_eq!(starts.len(), max_players, "{shape}: 可用格不足以安排首子");
    for (p, &(x, y)) in starts.iter().enumerate() {
        // 首子等级 = 「本格的」阈值 − 1。不能按 cap_mode 硬编码：
        // 降级边界下形状边缘的格子阈值是 3 或 2，写成 3 点会让首子直接超临界，
        // AI 无棋可走（get_moves 只收 count < 阈值 的己方格），对局 0 步就结束。
        let th = cell_threshold(&board, x, y, bm, cm);
        board[x][y] = Cell { owner: Some(p), count: th.saturating_sub(1) as u8, th: board[x][y].th, blocked: false };
    }
    let mut ai_configs = std::collections::HashMap::new();
    for (p, alg) in players.iter().enumerate() {
        ai_configs.insert(
            p.to_string(),
            serde_json::json!({
                "algorithm": alg,
                "depth": alg_depth(alg, cfg),
                "useMlEval": cfg.use_ml_eval,
                "randomScale": cfg.random_scale,
            }),
        );
    }
    if std::env::var("AI_BENCH_DEBUG").is_ok() {
        eprintln!("[{shape}/{bm:?}/{cm:?}] 首子={starts:?} 棋盘={rows}x{cols}");
        for row in &board {
            let line: String = row.iter().map(|c| match c.owner {
                Some(o) => char::from_digit(o as u32, 10).unwrap_or('?'),
                None if c.blocked => '#',
                None => '.',
            }).collect();
            eprintln!("   {line}");
        }
    }
    let r = simulate_to_end(
        board, rows, max_players, 0, vec![], bm, cm, None, gc, ai_configs,
    );
    if std::env::var("AI_BENCH_DEBUG").is_ok() {
        eprintln!("  -> winner={:?} steps={} order={:?}", r.winner, r.history.len(), r.eliminated_order);
    }
    (r.winner, r.history.len(), r.eliminated_order)
}

fn emit(obj: serde_json::Value) {
    let mut s = serde_json::to_string(&obj).unwrap_or_else(|_| "{\"error\":\"serialize\"}".into());
    s.push('\n');
    let _ = io::stdout().write_all(s.as_bytes());
    let _ = io::stdout().flush();
}

fn main() {
    let mut buf = String::new();
    if io::stdin().read_to_string(&mut buf).is_err() {
        emit(serde_json::json!({"fatal": "stdin read failed"}));
        std::process::exit(1);
    }
    let cfg: BenchConfig = match serde_json::from_str(&buf) {
        Ok(c) => c,
        Err(e) => {
            emit(serde_json::json!({"fatal": format!("配置解析失败: {}", e)}));
            std::process::exit(1);
        }
    };

    let mut id: u64 = 0;
    let cm_label = |cm: &CapMode| match cm {
        CapMode::Cap3 => "3", CapMode::Cap4 => "4", CapMode::Cap5 => "5", CapMode::Random => "random",
    };
    let shapes: Vec<String> = if cfg.shapes.is_empty() { default_shapes() } else { cfg.shapes.clone() };

    if cfg.mode == "duel" {
        let pairs: Vec<Vec<String>> = if !cfg.duel_pairs.is_empty() {
            cfg.duel_pairs.clone()
        } else {
            let mut v = Vec::new();
            for i in 0..cfg.algs.len() {
                for j in (i + 1)..cfg.algs.len() {
                    v.push(vec![cfg.algs[i].clone(), cfg.algs[j].clone()]);
                }
            }
            v
        };
        let total = shapes.len() as u64 * cfg.borders.len() as u64
            * cfg.caps.len() as u64 * pairs.len() as u64 * cfg.games as u64;
        emit(serde_json::json!({"meta": {"mode": "duel", "total": total, "pairs": pairs.len(), "shapes": shapes}}));
        for shape in &shapes {
            for bm in &cfg.borders {
                for cm in &cfg.caps {
                    for pair in &pairs {
                        for _ in 0..cfg.games {
                            let gc = cfg.seed.wrapping_add(id as u32);
                            let (winner, steps, order) = run_game(shape, cfg.size, *bm, *cm, gc, pair, &cfg);
                            emit(serde_json::json!({
                                "id": id, "shape": shape,
                                "bm": format!("{:?}", bm).to_lowercase(),
                                "cm": cm_label(cm),
                                "mode": "duel", "players": pair,
                                "winner": winner, "steps": steps, "order": order,
                            }));
                            id += 1;
                        }
                    }
                }
            }
        }
    } else {
        let players = cfg.algs.clone();
        let total = shapes.len() as u64 * cfg.borders.len() as u64 * cfg.caps.len() as u64 * cfg.games as u64;
        emit(serde_json::json!({"meta": {"mode": "ffa", "total": total, "players": players, "shapes": shapes}}));
        for shape in &shapes {
            for bm in &cfg.borders {
                for cm in &cfg.caps {
                    for _ in 0..cfg.games {
                        let gc = cfg.seed.wrapping_add(id as u32);
                        let (winner, steps, order) = run_game(shape, cfg.size, *bm, *cm, gc, &players, &cfg);
                        emit(serde_json::json!({
                            "id": id, "shape": shape,
                            "bm": format!("{:?}", bm).to_lowercase(),
                            "cm": cm_label(cm),
                            "mode": "ffa", "players": players,
                            "winner": winner, "steps": steps, "order": order,
                        }));
                        id += 1;
                    }
                }
            }
        }
    }
    emit(serde_json::json!({"meta": {"done": true, "ran": id}}));
}
