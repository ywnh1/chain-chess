// 连锁棋 AI 战力评估测试
//
// 用法（推荐直接跑默认赛程）:
//   cargo run --release --bin strength_test
//
// 可选参数:
//   --games N        每配对每模式局数（默认 6，偶数保证先手轮换对称）
//   --size N         棋盘大小（默认 7）
//   --depth N        alphabeta/pvs 搜索深度（默认 2）
//   --mcts-depth N   MCTS 深度（默认 1）
//   --caps 3,4,5     阈值模式列表（默认 3,4,5）
//   --borders a,b,c  边界模式列表（默认 default,wrap,bounce,degrade）
//   --algs a,b,c     参战算法（默认 strategy,alphabeta,pvs,mcts）
//   --ml / --no-ml   是否用 ML 评估（默认 --ml）
//   --seed N         随机种子（默认 42）
//
// 输出: 格式化报告（综合排名 + 配对明细 + 先手/后手拆解 + 性能）

use std::collections::HashMap;
use std::env;
use std::time::Instant;

use chain_chess_lib::{BorderMode, CapMode, Cell, GameBoard, simulate_to_end};

// ── 命令行参数 ──

struct Opts {
    games: u32,
    size: usize,
    depth: usize,
    mcts_depth: usize,
    caps: Vec<CapMode>,
    borders: Vec<BorderMode>,
    algs: Vec<String>,
    use_ml: bool,
    seed: u64,
}

fn parse_caps(s: &str) -> Vec<CapMode> {
    s.split(',').filter_map(|x| match x.trim() {
        "3" => Some(CapMode::Cap3),
        "4" => Some(CapMode::Cap4),
        "5" => Some(CapMode::Cap5),
        "random" => Some(CapMode::Random),
        _ => None,
    }).collect()
}

fn parse_borders(s: &str) -> Vec<BorderMode> {
    s.split(',').filter_map(|x| match x.trim() {
        "default" => Some(BorderMode::Default),
        "wrap" => Some(BorderMode::Wrap),
        "bounce" => Some(BorderMode::Bounce),
        "degrade" => Some(BorderMode::Degrade),
        _ => None,
    }).collect()
}

fn parse_algs(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

fn parse_opts() -> Opts {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut o = Opts {
        games: 6, size: 7, depth: 2, mcts_depth: 1,
        caps: parse_caps("3,4,5"),
        borders: parse_borders("default,wrap,bounce,degrade"),
        algs: parse_algs("strategy,alphabeta,pvs,mcts"),
        use_ml: true, seed: 42,
    };
    let mut i = 0;
    while i < args.len() {
        let get = |i: &mut usize| -> String { let v = args.get(*i + 1).cloned().unwrap_or_default(); *i += 2; v };
        match args[i].as_str() {
            "--games" => o.games = get(&mut i).parse().unwrap_or(6),
            "--size" => o.size = get(&mut i).parse().unwrap_or(7),
            "--depth" => o.depth = get(&mut i).parse().unwrap_or(2),
            "--mcts-depth" => o.mcts_depth = get(&mut i).parse().unwrap_or(1),
            "--caps" => o.caps = parse_caps(&get(&mut i)),
            "--borders" => o.borders = parse_borders(&get(&mut i)),
            "--algs" => o.algs = parse_algs(&get(&mut i)),
            "--ml" => { o.use_ml = true; i += 1; }
            "--no-ml" => { o.use_ml = false; i += 1; }
            "--seed" => o.seed = get(&mut i).parse().unwrap_or(42),
            other => { eprintln!("⚠️  未知参数: {other}（忽略）"); i += 1; }
        }
    }
    o
}

// ── 对局构造（首子均匀散布，消除首子位置偏差） ──

/// 合法首子布局：内圈均匀散布，彼此切比雪夫距离 ≥3
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
        while pos.iter().any(|&(px, py)| px.abs_diff(x).max(py.abs_diff(y)) < 3) && guard < 100 {
            x = (x + 3) % sz;
            y = (y + 2) % sz;
            guard += 1;
        }
        pos.push((x, y));
    }
    pos
}

fn build_board(sz: usize) -> GameBoard {
    vec![vec![Cell { owner: None, count: 0, th: None }; sz]; sz]
}

/// 跑一局，返回 (胜者, 步数)
fn run_game(
    sz: usize,
    bm: BorderMode,
    cm: CapMode,
    gc: u32,
    players: &[String],
    depth: usize,
    mcts_depth: usize,
    use_ml: bool,
) -> (Option<usize>, usize) {
    let max_players = players.len();
    let mut board = build_board(sz);
    let starts = spread_starts(sz, max_players);
    let th: u32 = match cm {
        CapMode::Cap3 => 3,
        CapMode::Cap5 => 5,
        CapMode::Random => 3,
        CapMode::Cap4 => 4,
    };
    for (p, &(x, y)) in starts.iter().enumerate() {
        board[x][y] = Cell { owner: Some(p), count: (th - 1) as u8, th: board[x][y].th };
    }
    let mut ai_configs = HashMap::new();
    for (p, alg) in players.iter().enumerate() {
        let d = if alg == "mcts" { mcts_depth } else { depth };
        ai_configs.insert(p.to_string(), serde_json::json!({
            "algorithm": alg, "depth": d, "useMlEval": use_ml, "randomScale": 0,
        }));
    }
    let r = simulate_to_end(board, sz, max_players, 0, vec![], bm, cm, None, gc, ai_configs);
    (r.winner, r.history.len())
}

// ── 统计 ──

#[derive(Default, Clone)]
struct PairStat {
    games: u32,
    wins: [u32; 2],   // 0=alg_a, 1=alg_b
    draws: u32,
    first_wins: [u32; 2], // 先手方胜利次数（记录先手是 A 还是 B）
    steps_sum: u64,
}

fn cap_label(cm: &CapMode) -> &'static str {
    match cm { CapMode::Cap3 => "cap3", CapMode::Cap4 => "cap4", CapMode::Cap5 => "cap5", CapMode::Random => "rand" }
}

fn border_label(bm: &BorderMode) -> &'static str {
    match bm { BorderMode::Default => "default", BorderMode::Wrap => "wrap", BorderMode::Bounce => "bounce", BorderMode::Degrade => "degrade", BorderMode::Random => "rand" }
}

fn main() {
    let o = parse_opts();
    if o.algs.len() < 2 {
        eprintln!("❌ 至少需要 2 个算法参战（--algs）");
        std::process::exit(1);
    }
    if o.games % 2 != 0 {
        eprintln!("⚠️  --games 建议用偶数（保证先手轮换对称），继续使用 {}", o.games);
    }

    // 配对：全组合
    let mut pairs: Vec<(String, String)> = Vec::new();
    for i in 0..o.algs.len() {
        for j in (i + 1)..o.algs.len() {
            pairs.push((o.algs[i].clone(), o.algs[j].clone()));
        }
    }

    let total_games = pairs.len() as u64 * o.borders.len() as u64 * o.caps.len() as u64 * o.games as u64;
    let mut stats: HashMap<(String, String, String, String), PairStat> = HashMap::new();
    let start = Instant::now();
    let mut done: u64 = 0;

    println!("════════════════════════════════════════════════════════════");
    println!("  连锁棋 AI 战力评估报告");
    println!("  棋盘 {}×{} · alphabeta/pvs 深度 {} · MCTS 深度 {} · {}评估",
        o.size, o.size, o.depth, o.mcts_depth, if o.use_ml { "ML" } else { "手写" });
    println!("  参战: {} · 每配对 {} 局/模式（先手轮换） · seed {}",
        o.algs.join(" / "), o.games, o.seed);
    println!("  赛程: {} 配对 × {} 边界 × {} 阈值 × {} 局 = {} 局",
        pairs.len(), o.borders.len(), o.caps.len(), o.games, total_games);
    println!("════════════════════════════════════════════════════════════");
    println!();

    for (ai, (alg_a, alg_b)) in pairs.iter().enumerate() {
        for bm in &o.borders {
            for cm in &o.caps {
                let key = (alg_a.clone(), alg_b.clone(), border_label(bm).to_string(), cap_label(cm).to_string());
                let st = stats.entry(key).or_default();
                for g in 0..o.games {
                    // 先手轮换：偶数局 A 先手，奇数局 B 先手
                    let (first, second) = if g % 2 == 0 { (alg_a.as_str(), alg_b.as_str()) } else { (alg_b.as_str(), alg_a.as_str()) };
                    let gc = o.seed.wrapping_add((g as u64) * 7919).wrapping_add((ai as u64) * 131).wrapping_add(done as u32 as u64);
                    let (winner, steps) = run_game(o.size, *bm, *cm, gc as u32, &[first.to_string(), second.to_string()], o.depth, o.mcts_depth, o.use_ml);
                    let a_won = winner.map(|w| w == 0).unwrap_or(false) && first == alg_a
                        || winner.map(|w| w == 1).unwrap_or(false) && first == alg_b;
                    let b_won = !a_won && winner.is_some();
                    st.games += 1;
                    if a_won { st.wins[0] += 1; } else if b_won { st.wins[1] += 1; } else { st.draws += 1; }
                    // 先手拆解：先手方是谁且是否获胜
                    let first_is_a = first == alg_a;
                    let first_won = winner.map(|w| w == 0).unwrap_or(false); // 先手玩家索引恒为 0
                    if first_won { st.first_wins[if first_is_a { 0 } else { 1 }] += 1; }
                    st.steps_sum += steps as u64;
                    done += 1;
                    if done % 25 == 0 || done == total_games {
                        let pct = done as f64 / total_games as f64 * 100.0;
                        let eta = start.elapsed().as_secs_f64() / done as f64 * (total_games - done) as f64;
                        eprintln!("  ⏳ 进度 {:>3.0}%  ({}/{} 局)  ETA {}s", pct, done, total_games, eta as u64);
                    }
                }
            }
        }
    }

    // ── 综合排名：跨模式聚合每个算法胜率 ──
    println!("── 综合排名（所有模式汇总） ───────────────────────────────");
    println!("  {:<12} {:>6} {:>6} {:>6} {:>8} {:>10}", "算法", "胜", "负", "平", "胜率", "均步");
    let mut agg: HashMap<String, (u32, u32, u32, u64, u32)> = HashMap::new(); // alg -> (w,l,d,steps,games)
    for ((a, b, _, _), st) in &stats {
        let e = agg.entry(a.clone()).or_default();
        e.0 += st.wins[0]; e.1 += st.wins[1]; e.2 += st.draws; e.3 += st.steps_sum; e.4 += st.games;
        let e2 = agg.entry(b.clone()).or_default();
        e2.0 += st.wins[1]; e2.1 += st.wins[0]; e2.2 += st.draws; e2.3 += st.steps_sum; e2.4 += st.games;
    }
    let mut ranking: Vec<(&String, &(u32, u32, u32, u64, u32))> = agg.iter().collect();
    ranking.sort_by(|x, y| {
        let wr = |e: &(u32, u32, u32, u64, u32)| if e.4 == 0 { 0.0 } else { e.0 as f64 / e.4 as f64 };
        wr(y.1).partial_cmp(&wr(x.1)).unwrap()
    });
    for (i, (alg, (w, l, d, steps, games))) in ranking.iter().enumerate() {
        let wr = if *games == 0 { 0.0 } else { *w as f64 / *games as f64 * 100.0 };
        let avg = if *games == 0 { 0.0 } else { *steps as f64 / *games as f64 };
        println!("  #{:<2} {:<10} {:>6} {:>6} {:>6} {:>7.1}% {:>10.1}", i + 1, alg, w, l, d, wr, avg);
    }
    println!();

    // ── 配对明细 ──
    println!("── 配对明细（A 先手/后手轮换，按模式） ─────────────────────");
    for (_ai, (alg_a, alg_b)) in pairs.iter().enumerate() {
        println!("  ◆ {} vs {}（先手轮换 {}/{}）", alg_a, alg_b, 1, 2);
        for bm in &o.borders {
            for cm in &o.caps {
                let key = (alg_a.clone(), alg_b.clone(), border_label(bm).to_string(), cap_label(cm).to_string());
                let st = stats.get(&key).unwrap();
                let fw = st.first_wins[0] + st.first_wins[1];
                println!("     {:<8} {:<8}  {:>2}:{:<2} (平{})  先手胜 {:>2}/{:>2}  均步 {:>4.0}",
                    border_label(bm), cap_label(cm),
                    st.wins[0], st.wins[1], st.draws,
                    fw, st.games,
                    st.steps_sum as f64 / st.games.max(1) as f64);
            }
        }
        println!();
    }

    let elapsed = start.elapsed();
    println!("── 性能 ──────────────────────────────────────────────────");
    println!("  总对局: {}  总耗时: {:.1}s  平均 {:.2}s/局",
        done, elapsed.as_secs_f64(), elapsed.as_secs_f64() / done.max(1) as f64);
    println!("════════════════════════════════════════════════════════════");
    println!("  ✅ 评估完成。调整赛程: --games / --size / --caps / --borders / --algs / --no-ml");
}
