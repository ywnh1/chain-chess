// 连锁棋 ML vs 手写 AI 胜负对比基准测试
// 用法: cargo run --release --bin battle -- <棋盘大小> <深度> <局数>
//
// 示例: cargo run --release --bin battle -- 7 2 20

use std::env;
use std::time::Instant;
use chain_chess_lib::*;

struct BattleOutcome {
    ml_wins: u32,
    hand_wins: u32,
    draws: u32,
    ml_first: u32,
    hand_first: u32,
}

fn play_game(
    board_size: usize,
    max_players: usize,
    depth: usize,
    ml_goes_first: bool,
    game_id: u32,
) -> Option<usize> {
    let mut board = vec![vec![Cell { owner: None, count: 0 }; board_size]; board_size];
    let mut eliminated: Vec<usize> = Vec::new();
    let mut cur_player: usize = 0;

    let ml_player = if ml_goes_first { 0 } else { 1 };
    let _hand_player = if ml_goes_first { 1 } else { 0 };

    loop {
        let legal_moves = get_moves(&board, board_size, cur_player, None);
        if legal_moves.is_empty() {
            // No legal moves - find alive players
            let alive: Vec<usize> = (0..max_players)
                .filter(|p| !eliminated.contains(p))
                .collect();
            if alive.len() <= 1 { break; }
            let idx = alive.iter().position(|&p| p == cur_player).unwrap_or(0);
            cur_player = alive[(idx + 1) % alive.len()];
            continue;
        }

        // Check if this player should use ML or handcraft eval
        // 用 alphabeta 而非 pvs，避免 quiescence 搜索爆炸
        let config = if cur_player == ml_player {
            PlayerAiConfig { algorithm: "alphabeta".to_string(), depth, use_ml_eval: true }
        } else {
            PlayerAiConfig { algorithm: "alphabeta".to_string(), depth, use_ml_eval: false }
        };

        let chosen = find_best_move_by_alg(
            &board, board_size, cur_player, &config,
            &eliminated, max_players, game_id, None,
        );

        let (mx, my) = chosen.unwrap_or_else(|| legal_moves[0]);

        let (new_elim, _) = process_click(&mut board, board_size, mx, my, cur_player, max_players);
        for &e in &new_elim {
            if !eliminated.contains(&e) { eliminated.push(e); }
        }

        let alive_now: Vec<usize> = (0..max_players)
            .filter(|p| !eliminated.contains(p))
            .collect();
        if alive_now.len() <= 1 {
            break;
        }

        let idx = alive_now.iter().position(|&p| p == cur_player).unwrap_or(0);
        cur_player = alive_now[(idx + 1) % alive_now.len()];
    }

    // Determine winner
    let alive: Vec<usize> = (0..max_players)
        .filter(|p| !eliminated.contains(p) && has_pieces(&board, *p))
        .collect();
    alive.first().copied()
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let board_size = if args.len() > 1 { args[1].parse().unwrap_or(7) } else { 7 };
    let depth = if args.len() > 2 { args[2].parse().unwrap_or(2) } else { 2 };
    let num_games = if args.len() > 3 { args[3].parse().unwrap_or(10) } else { 10 };

    eprintln!("╔══════════════════════════════════════════╗");
    eprintln!("║    ML AI vs 手写评估 胜负对比测试        ║");
    eprintln!("╚══════════════════════════════════════════╝");
    eprintln!("  棋盘: {}×{}", board_size, board_size);
    eprintln!("  深度: {}", depth);
    eprintln!("  局数: {} (先手交换)", num_games);
    eprintln!("  AI: 双方均使用 PVS 搜索，仅评估函数不同");

    let mut outcome = BattleOutcome {
        ml_wins: 0, hand_wins: 0, draws: 0,
        ml_first: 0, hand_first: 0,
    };

    let start = Instant::now();

    for game_id in 0..num_games {
        // 先手交换：偶数局 ML 先手，奇数局手写先手
        let ml_first = game_id % 2 == 0;
        eprint!("  游戏 {}/{} (ML {}先手): ",
            game_id + 1, num_games,
            if ml_first { "" } else { "后" });

        let winner = play_game(board_size, 2, depth, ml_first, game_id as u32);

        match winner {
            Some(w) if (w == 0 && ml_first) || (w == 1 && !ml_first) => {
                outcome.ml_wins += 1;
                if ml_first { outcome.ml_first += 1; }
                eprintln!("ML 胜");
            }
            Some(_w) => {
                outcome.hand_wins += 1;
                if !ml_first { outcome.hand_first += 1; }
                eprintln!("手写胜");
            }
            None => {
                outcome.draws += 1;
                eprintln!("平局");
            }
        }
    }

    let elapsed = start.elapsed();
    let avg_time = elapsed.as_secs_f64() / num_games as f64;

    eprintln!();
    eprintln!("═══════════ 测试结果 ═══════════");
    eprintln!("  ML 胜:    {} / {} ({:.0}%)", outcome.ml_wins, num_games,
        outcome.ml_wins as f64 / num_games as f64 * 100.0);
    eprintln!("  手写胜:  {} / {} ({:.0}%)", outcome.hand_wins, num_games,
        outcome.hand_wins as f64 / num_games as f64 * 100.0);
    if outcome.draws > 0 {
        eprintln!("  平局:    {}", outcome.draws);
    }
    eprintln!("  总耗时:  {:.1}s, 平均 {:.1}s/局", elapsed.as_secs_f64(), avg_time);
    eprintln!();

    // 输出结果（脚本可解析）
    println!("RESULT: ML={} HAND={} DRAW={} TOTAL={} TIME={:.1}s",
        outcome.ml_wins, outcome.hand_wins, outcome.draws, num_games, elapsed.as_secs_f64());
}
