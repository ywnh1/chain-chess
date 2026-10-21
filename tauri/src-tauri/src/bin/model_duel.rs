// 连锁棋双模型对战（实验工具）
//
// 用途：让两个 XGBoost 模型在同一套搜索下对打，比较胜率 —— 用于验证重训后的
// 模型是否真的更强，尤其是在新支持的异形棋盘上。
//
// 为什么不用 strength_test / battle：引擎里的模型是 include_str! 编译期内嵌的
// （xgb_engine() 那个 OnceLock），一个进程只能用一份。要两个模型同时上场，
// 只能绕开内嵌路径，用 get_xgb_engine_for_test 在运行时分别加载两份。
//
// 搜索是这层自己实现的 negamax（两方共用同一套逻辑、同一深度），
// 所以胜率差异只可能来自模型本身，不掺搜索强度的差异。
//
// 用法:
//   cargo run --release --bin model_duel -- <模型A> <模型B> [局数] [行] [列] [洞宽] [深度]
// 例:
//   cargo run --release --bin model_duel -- /tmp/old.json engine/xgb_model_board.json 15 7 7 0

use std::env;
use std::time::Instant;

use chain_chess_core::{
    extract_features_improved, get_moves, get_xgb_engine_for_test, has_pieces, max_players_for,
    order_moves, process_click, xgb_predict, BorderMode, CapMode, Cell, GameBoard, XGBoostEngine,
};

const BORDER: BorderMode = BorderMode::Default;
const CAP: CapMode = CapMode::Cap4;
const MAX_PLAYERS: usize = 2;
/// 每层最多展开几个候选走法（真实 AI 也做同样的剪枝，避免这里退化成全宽搜索）
const MAX_CAND: usize = 10;
const WIN: f32 = 1e6;
const MAX_STEPS: usize = 1500;

fn make_board(rows: usize, cols: usize, hole_width: usize) -> GameBoard {
    (0..rows)
        .map(|_| {
            (0..cols)
                .map(|j| if j < hole_width || j + hole_width >= cols { Cell::hole() } else { Cell::empty() })
                .collect()
        })
        .collect()
}

/// 开局给双方各放一颗子（关于中心列对称）。
///
/// 不能空盘开局：那样任何一方落第一步后，对方手上都还是 0 颗子，
/// 「对手没有棋子」就等价于「被吃光」—— 会立刻误判先手获胜，
/// 实测表现为每局都是先手赢、胜负与棋盘形状完全无关。
fn seed_opening(board: &mut GameBoard, rows: usize, cols: usize, seed: u64) {
    let mi = rows / 2;
    let mj = cols / 2;
    let free = |b: &GameBoard, i: usize, j: usize| !b[i][j].blocked && b[i][j].owner.is_none();
    // 开局距离也随机。否则每局初始局面完全相同，而搜索又是确定性的
    // （Cap4 + 默认边界不消耗随机数，传进去的 seed 根本没起作用），
    // 结果是「每 4 局一个固定模式」的重复 —— 15 局实际只等于 4 种组合，
    // 实测换 5 个种子结果一模一样。
    // span 只有 cols/2 种（7 列 → 3 种），太窄：实测不同种子常常抽出同一开局，
    // 再叠加搜索的确定性，直接导致「每 3 局一个循环」的重复对局。
    // 这里把 seed 当 64 位熵用：距离 d 与行 mi 都随它变，取模仍保证对称。
    let span = (cols / 2).max(1);
    let start = 1 + (seed % span as u64) as usize;
    let row_off = ((seed / span as u64) % rows.max(1) as u64) as usize;
    for k in 0..span {
        let d = 1 + ((start - 1 + k) % span);
        let r_i = (mi + row_off) % rows.max(1);
        if mj < d || mj + d >= cols { continue; }
        let (l, r) = (mj - d, mj + d);
        if free(board, r_i, l) && free(board, r_i, r) {
            board[r_i][l].owner = Some(0);
            board[r_i][l].count = 1;
            board[r_i][r].owner = Some(1);
            board[r_i][r].count = 1;
            return;
        }
    }
}

/// 从 player 视角给局面打分（XGBoost 的 log-odds，越大越好）
fn eval(engine: &XGBoostEngine, board: &GameBoard, player: usize) -> f32 {
    let mp = max_players_for(board);
    let feats = extract_features_improved(board, player, mp, BORDER, CAP);
    xgb_predict(engine, &feats).0
}

/// 负极大值搜索，返回从 player 视角的分数
fn negamax(engine: &XGBoostEngine, board: &GameBoard, rows: usize, player: usize, depth: usize, seed: u64) -> f32 {
    let opp = 1 - player;
    // 终局：对方棋子被清空即获胜（减 depth 让「快点赢」优先）
    if !has_pieces(board, opp) { return WIN - depth as f32; }
    if !has_pieces(board, player) { return -WIN + depth as f32; }
    if depth == 0 { return eval(engine, board, player); }

    let moves = get_moves(board, rows, player, None, BORDER, CAP);
    if moves.is_empty() { return -WIN + depth as f32; }   // 无棋可走视同判负

    let ordered = order_moves(moves, board, player, BORDER, CAP);
    let mut best = f32::NEG_INFINITY;
    for (i, &(x, y)) in ordered.iter().take(MAX_CAND).enumerate() {
        let mut nb = board.clone();
        process_click(&mut nb, rows, x, y, player, MAX_PLAYERS, BORDER, CAP, Some(seed + i as u64));
        let v = -negamax(engine, &nb, rows, opp, depth - 1, seed.wrapping_add(7919));
        if v >= best { best = v; }
    }
    best
}

/// 选出让当前玩家分数最高的走法
fn choose_move(
    engine: &XGBoostEngine,
    board: &GameBoard,
    rows: usize,
    player: usize,
    depth: usize,
    seed: u64,
) -> Option<(usize, usize)> {
    let moves = get_moves(board, rows, player, None, BORDER, CAP);
    if moves.is_empty() { return None; }
    let ordered = order_moves(moves, board, player, BORDER, CAP);
    let opp = 1 - player;

    let mut best: Option<(f32, (usize, usize))> = None;
    for (i, &(x, y)) in ordered.iter().take(MAX_CAND).enumerate() {
        let mut nb = board.clone();
        process_click(&mut nb, rows, x, y, player, MAX_PLAYERS, BORDER, CAP, Some(seed + i as u64));
        let v = if !has_pieces(&nb, opp) {
            WIN
        } else {
            -negamax(engine, &nb, rows, opp, depth.saturating_sub(1), seed.wrapping_add(7919))
        };
        if best.map_or(true, |(bv, _)| v >= bv) { best = Some((v, (x, y))); }
    }
    best.map(|(_, m)| m)
}

/// 跑一局，返回胜者（None = 超过步数上限，算平局）
fn play_one(
    engines: [&XGBoostEngine; 2],
    rows: usize,
    cols: usize,
    hole_width: usize,
    first: usize,
    depth: usize,
    seed: u64,
    verbose: bool,
) -> Option<usize> {
    let mut board = make_board(rows, cols, hole_width);
    seed_opening(&mut board, rows, cols, seed);
    let mut cur = first;
    let mut result = None;
    let mut taken = 0usize;
    for step in 0..MAX_STEPS {
        taken = step + 1;
        let s = seed.wrapping_add(step as u64 * 104729);
        let Some((x, y)) = choose_move(engines[cur], &board, rows, cur, depth, s) else {
            result = Some(1 - cur);   // 有子却无棋可走：死锁，判负
            break;
        };
        // 用引擎返回的 eliminated 判胜负 —— 它按「曾经有子、现在被吃光」判定，
        // 比只看 has_pieces 可靠
        let (elim, _) = process_click(&mut board, rows, x, y, cur, MAX_PLAYERS, BORDER, CAP, Some(s));
        let other = 1 - cur;
        if elim.contains(&other) { result = Some(cur); break; }
        if elim.contains(&cur) { result = Some(other); break; }
        // 兜底：引擎没报淘汰但棋子确实没了（理论上不该发生）
        if !has_pieces(&board, other) { result = Some(cur); break; }
        if !has_pieces(&board, cur) { result = Some(other); break; }
        cur = other;
    }
    if verbose {
        eprintln!("    [诊断] 走了 {} 步，结束时 P0 子 {} / P1 子 {}",
            taken, count_pieces(&board, 0), count_pieces(&board, 1));
    }
    result
}

fn count_pieces(board: &GameBoard, p: usize) -> usize {
    board.iter().flatten().filter(|c| c.owner == Some(p)).count()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("用法: {} <模型A> <模型B> [局数] [行] [列] [洞宽] [深度]", args[0]);
        eprintln!("例:   {} /tmp/old.json engine/xgb_model_board.json 15 7 7 0", args[0]);
        std::process::exit(2);
    }
    let path_a = &args[1];
    let path_b = &args[2];
    let games: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(15);
    let rows: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(7);
    let cols: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(7);
    let hole_width: usize = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(0);
    let depth: usize = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(2);
    let seed: u64 = args.get(8).and_then(|s| s.parse().ok()).unwrap_or(20261011);
    let verbose = env::var("DUEL_VERBOSE").is_ok();

    println!("┌────────────────────────────────────────────────┐");
    println!("│   连锁棋 双模型对战                            │");
    println!("└────────────────────────────────────────────────┘");
    println!("  A : {}", path_a);
    println!("  B : {}", path_b);
    println!("  局数 : {}（先手每局轮换）", games);
    let shape = if hole_width == 0 {
        if rows == cols { format!("正方形 {rows}×{cols}") } else { format!("长方形 {rows}×{cols}") }
    } else {
        format!("不规则 {rows}×{cols}（左右各挖 {hole_width} 列）")
    };
    println!("  棋盘 : {}", shape);
    println!("  搜索 : negamax 深度 {depth}，每层最多 {MAX_CAND} 个候选");
    println!();

    let eng_a = get_xgb_engine_for_test(path_a);
    let eng_b = get_xgb_engine_for_test(path_b);

    let start = Instant::now();
    let (mut win_a, mut win_b, mut draw) = (0usize, 0usize, 0usize);
    // 角色偏差诊断：若 P0 系统性占优，A/B 轮流当 P0 会把结果系统性拉向对半，
    // 掩盖真实的模型强度差。必须把它单独统计出来。
    let (mut p0_wins, mut p1_wins) = (0usize, 0usize);
    // 分组统计：A 执 P0 时 A 的胜率 / B 执 P0 时 B 的胜率。
    // 混在一起的「A 胜率」会被轮换设计抹平（A/B 各半 P0，而胜负主要看角色），
    // 但分开看这两栏，强模型在两栏里都更高 —— 这才是能测出强度的指标。
    let (mut a_p0_games, mut a_p0_wins, mut b_p0_games, mut b_p0_wins) = (0usize, 0usize, 0usize, 0usize);
    for g in 0..games {
        // 配对设计：每 4 局为一组，组内（slot 0/1、slot 2/3）用同一个开局 seed，
        // 让 A/B 在完全相同开局下各执一次 P0、各当一次先手。
        //
        // 为什么必须配对：非配对时每个开局只采样一次，开局本身的方差会直接灌进
        // 结论 —— 实测旧模型自己打自己（同一文件）能偏到 4:13，且"同是 P0"的两组
        // 胜率一个 44% 一个 100%，差异全部来自开局而不是模型。
        let quad = g / 4;
        let slot = g % 4;
        let a_is_p0 = slot % 2 == 0;
        let first = (slot / 2) % 2;
        let arr: [&XGBoostEngine; 2] = if a_is_p0 { [&eng_a, &eng_b] } else { [&eng_b, &eng_a] };
        let winner = play_one(arr, rows, cols, hole_width, first, depth, seed.wrapping_add(quad as u64 * 7919), verbose);
        let tag = match winner {
            Some(p) => {
                if p == 0 { p0_wins += 1 } else { p1_wins += 1 }
                if a_is_p0 {
                    a_p0_games += 1;
                    if p == 0 { a_p0_wins += 1 }
                } else {
                    b_p0_games += 1;
                    if p == 0 { b_p0_wins += 1 }
                }
                if (p == 0) == a_is_p0 { win_a += 1; "A 胜" } else { win_b += 1; "B 胜" }
            }
            None => { draw += 1; "平局" }
        };
        let first_side = if (first == 0) == a_is_p0 { "A" } else { "B" };
        println!("  第 {:>2} 局  先手 {}  →  {}", g + 1, first_side, tag);
    }

    let decided = win_a + win_b;
    println!();
    println!("────────────────────────────────────────────────");
    println!("  A 胜 {} 局 / B 胜 {} 局 / 平 {} 局", win_a, win_b, draw);
    let role_decided = p0_wins + p1_wins;
    if role_decided > 0 {
        println!("  [角色诊断] P0 胜 {} 局 / P1 胜 {} 局 → P0 胜率 {:.1}%（应为 50%，越高偏差越大）",
            p0_wins, p1_wins, p0_wins as f64 * 100.0 / role_decided as f64);
    }
    // 关键指标：同一角色下两个模型的胜率。两者差距 = 可测量的强度差。
    let rate = |w: usize, g: usize| if g == 0 { f64::NAN } else { w as f64 * 100.0 / g as f64 };
    if a_p0_games > 0 && b_p0_games > 0 {
        let (ra, rb) = (rate(a_p0_wins, a_p0_games), rate(b_p0_wins, b_p0_games));
        println!("  [强度对比] A 执 P0: {}/{} = {:.1}%  |  B 执 P0: {}/{} = {:.1}%  →  强度差 {:+.1} 个百分点",
            a_p0_wins, a_p0_games, ra, b_p0_wins, b_p0_games, rb, ra - rb);
    }
    if decided > 0 {
        println!("  有效对局 {} 局，A 胜率 {:.1}% / B 胜率 {:.1}%",
            decided,
            win_a as f64 * 100.0 / decided as f64,
            win_b as f64 * 100.0 / decided as f64);
    } else {
        println!("  没有分出胜负的对局");
    }
    println!("  耗时 {:.1}s", start.elapsed().as_secs_f64());
}
