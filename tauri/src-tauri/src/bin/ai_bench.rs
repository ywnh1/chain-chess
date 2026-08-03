// 连锁棋 AI 实力基准（供 ai_strength.py 调度）
//
// 从 stdin 读取一个 JSON 配置，逐局用 tauri 端引擎 simulate_to_end 模拟 AI 对局，
// stdout 每行输出一个 JSON 结果（行协议），Python 端实时读取并展示进度条。
//
// 配置格式（JSON）:
// {
//   "size": 7, "games": 6, "mode": "ffa" | "duel",
//   "borders": ["default","wrap","bounce","degrade"],
//   "caps": ["3","4","5","mixed"],
//   "algs": ["strategy","alphabeta","pvs","mcts"],   // ffa 模式每玩家一算法
//   "duelPairs": [["strategy","alphabeta"]],          // duel 模式（缺省自动全配对）
//   "depth": 2, "mctsDepth": 1, "randomScale": 5, "useMlEval": true,
//   "seed": 0
// }
//
// 输出（每行一个）:
// {"id":0,"bm":"default","cm":"4","mode":"ffa","players":[...],"winner":0,"steps":123,"order":[2,3,1]}
//
// 用法: cargo run --release --bin ai_bench < config.json
use std::io::{self, Read, Write};
use serde::Deserialize;
use chain_chess_lib::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BenchConfig {
    #[serde(default = "default_size")]
    size: usize,
    #[serde(default = "default_games")]
    games: u32,
    #[serde(default = "default_mode")]
    mode: String,
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
fn default_borders() -> Vec<BorderMode> {
    vec![BorderMode::Default, BorderMode::Wrap, BorderMode::Bounce, BorderMode::Degrade]
}
fn default_caps() -> Vec<CapMode> {
    vec![CapMode::Cap3, CapMode::Cap4, CapMode::Cap5, CapMode::Mixed]
}
fn default_algs() -> Vec<String> {
    vec!["strategy".into(), "alphabeta".into(), "pvs".into(), "mcts".into()]
}
fn default_depth() -> usize { 2 }
fn default_mcts_depth() -> usize { 1 }
fn default_random() -> u32 { 5 }
fn default_ml() -> bool { true }

/// 合法首子布局：内圈均匀散布，彼此切比雪夫距离 ≥3（符合「首子 12 格限制」）
fn spread_starts(sz: usize, n: usize) -> Vec<(usize, usize)> {
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

/// 构造开局棋盘：mixed 模式按 seed 确定性分配每格阈值 3/4/5，然后放首子
fn build_board(sz: usize, cm: CapMode, gc: u32) -> GameBoard {
    let mut seed = gc;
    let mut rnd = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) % 3
    };
    let mixed = cm == CapMode::Mixed;
    let board = (0..sz)
        .map(|_| (0..sz)
            .map(|_| Cell { owner: None, count: 0, th: if mixed { Some(3 + rnd() as u8) } else { None } })
            .collect())
        .collect::<GameBoard>();
    board
}

fn alg_depth(alg: &str, cfg: &BenchConfig) -> usize {
    if alg == "mcts" { cfg.mcts_depth } else { cfg.depth }
}

/// 跑一局，返回 (winner, steps, eliminated_order)
fn run_game(
    sz: usize,
    bm: BorderMode,
    cm: CapMode,
    gc: u32,
    players: &[String],
    cfg: &BenchConfig,
) -> (Option<usize>, usize, Vec<usize>) {
    let max_players = players.len();
    let mut board = build_board(sz, cm, gc);
    let starts = spread_starts(sz, max_players);
    for (p, &(x, y)) in starts.iter().enumerate() {
        // 首子等级 = 阈值 n-1（cap3→2、cap4→3、cap5→4、mixed→所在格阈值-1）
        let th: u32 = match cm {
            CapMode::Cap3 => 3,
            CapMode::Cap4 => 4,
            CapMode::Cap5 => 5,
            CapMode::Mixed => board[x][y].th.unwrap_or(4) as u32,
            CapMode::Random => 3, // 每步随机阈值模式：开局首子取中间等级
        };
        board[x][y] = Cell { owner: Some(p), count: (th - 1) as u8, th: board[x][y].th };
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
    let r = simulate_to_end(
        board, sz, max_players, 0, vec![], bm, cm, None, gc, ai_configs,
    );
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
    // 预计算任务数（用于总进度提示，可跨进程约定）
    // ffa：borders × caps × games
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
        let task_count = cfg.borders.len() as u64 * cfg.caps.len() as u64 * pairs.len() as u64 * cfg.games as u64;
        emit(serde_json::json!({"meta": {"mode": "duel", "total": task_count, "pairs": pairs.len()}}));
        for bm in &cfg.borders {
            for cm in &cfg.caps {
                for pair in &pairs {
                    for _ in 0..cfg.games {
                        let gc = cfg.seed.wrapping_add(id as u32);
                        let (winner, steps, order) = run_game(cfg.size, *bm, *cm, gc, pair, &cfg);
                        emit(serde_json::json!({
                            "id": id, "bm": format!("{:?}", bm).to_lowercase(),
                            "cm": match cm { CapMode::Cap3 => "3", CapMode::Cap4 => "4", CapMode::Cap5 => "5", CapMode::Mixed => "mixed", CapMode::Random => "random" },
                            "mode": "duel", "players": pair,
                            "winner": winner, "steps": steps, "order": order,
                        }));
                        id += 1;
                    }
                }
            }
        }
    } else {
        let players = cfg.algs.clone();
        let task_count = cfg.borders.len() as u64 * cfg.caps.len() as u64 * cfg.games as u64;
        emit(serde_json::json!({"meta": {"mode": "ffa", "total": task_count, "players": players}}));
        for bm in &cfg.borders {
            for cm in &cfg.caps {
                for _ in 0..cfg.games {
                    let gc = cfg.seed.wrapping_add(id as u32);
                    let (winner, steps, order) = run_game(cfg.size, *bm, *cm, gc, &players, &cfg);
                    emit(serde_json::json!({
                        "id": id,
                        "bm": format!("{:?}", bm).to_lowercase(),
                        "cm": match cm { CapMode::Cap3 => "3", CapMode::Cap4 => "4", CapMode::Cap5 => "5", CapMode::Mixed => "mixed", CapMode::Random => "random" },
                        "mode": "ffa", "players": players,
                        "winner": winner, "steps": steps, "order": order,
                    }));
                    id += 1;
                }
            }
        }
    }
    emit(serde_json::json!({"meta": {"done": true, "ran": id}}));
}
