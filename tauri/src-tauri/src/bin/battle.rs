// 连锁棋 AI 对决基准测试
// 从 JSON 配置文件中读取对局信息
//
// 用法: cargo run --release --bin battle -- battle_config.json
//
// 配置文件格式:
// {
//   "size": 7,            // 棋盘大小
//   "times": 10,          // 对局数
//   "ai": [
//     {"type":"alphabeta","depth":2,"use_ml_eval":true,"name":"ML"},
//     {"type":"alphabeta","depth":2,"use_ml_eval":false,"name":"手写"}
//   ]
// }

use std::env;
use std::fs;
use std::time::Instant;
use serde::Deserialize;
use chain_chess_lib::*;

/// AI 选手配置（来自 JSON）
#[derive(Deserialize, Clone)]
struct AiPlayerConfig {
    #[serde(rename = "type")]
    alg: String,
    depth: Option<usize>,
    #[serde(default = "default_true")]
    use_ml_eval: bool,
    #[serde(default)]
    name: String,
}

fn default_true() -> bool { true }

/// 整场比赛配置
#[derive(Deserialize)]
struct BattleConfig {
    size: usize,
    times: u32,
    ai: Vec<AiPlayerConfig>,
}

fn play_game(
    board_size: usize,
    max_players: usize,
    configs: &[PlayerAiConfig; 2],
    first_player: usize,
    game_id: u32,
) -> Option<usize> {
    let mut board = vec![vec![Cell { owner: None, count: 0 }; board_size]; board_size];
    let mut eliminated: Vec<usize> = Vec::new();
    let mut cur_player: usize = first_player;

    loop {
        let legal_moves = get_moves(&board, board_size, cur_player, None);
        if legal_moves.is_empty() {
            let alive: Vec<usize> = (0..max_players)
                .filter(|p| !eliminated.contains(p))
                .collect();
            if alive.len() <= 1 { break; }
            let idx = alive.iter().position(|&p| p == cur_player).unwrap_or(0);
            cur_player = alive[(idx + 1) % alive.len()];
            continue;
        }

        let cfg = &configs[cur_player];
        let chosen = find_best_move_by_alg(
            &board, board_size, cur_player, cfg,
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
        if alive_now.len() <= 1 { break; }

        let idx = alive_now.iter().position(|&p| p == cur_player).unwrap_or(0);
        cur_player = alive_now[(idx + 1) % alive_now.len()];
    }

    let alive: Vec<usize> = (0..max_players)
        .filter(|p| !eliminated.contains(p) && has_pieces(&board, *p))
        .collect();
    alive.first().copied()
}

fn alg_label(alg: &str, depth: Option<usize>, use_ml: bool) -> String {
    let depth_str = match depth {
        Some(d) => format!(" depth={}", d),
        None => String::new(),
    };
    let ml_str = if use_ml { " ML" } else { " 手写" };
    match alg {
        "strategy" => "策略".to_string(),
        "mcts" => format!("MCTS{}", depth_str),
        _ => format!("{}{}{}", alg, depth_str, ml_str),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: cargo run --release --bin battle -- <config.json>");
        eprintln!("示例: cargo run --release --bin battle -- battle_config.json");
        std::process::exit(1);
    }

    let config_path = &args[1];
    let content = fs::read_to_string(config_path)
        .unwrap_or_else(|e| { eprintln!("❌ 无法读取配置: {}", e); std::process::exit(1); });
    let cfg: BattleConfig = serde_json::from_str(&content)
        .unwrap_or_else(|e| { eprintln!("❌ JSON 解析失败: {}", e); std::process::exit(1); });

    if cfg.ai.len() != 2 {
        eprintln!("❌ 必须有且只有 2 个 AI 选手 (当前 {} 个)", cfg.ai.len());
        std::process::exit(1);
    }

    let p0 = &cfg.ai[0];
    let p1 = &cfg.ai[1];
    let name0 = if p0.name.is_empty() { alg_label(&p0.alg, p0.depth, p0.use_ml_eval) } else { p0.name.clone() };
    let name1 = if p1.name.is_empty() { alg_label(&p1.alg, p1.depth, p1.use_ml_eval) } else { p1.name.clone() };

    let ai_configs: [PlayerAiConfig; 2] = [
        PlayerAiConfig {
            algorithm: p0.alg.clone(),
            depth: p0.depth.unwrap_or(0),
            use_ml_eval: p0.use_ml_eval,
        },
        PlayerAiConfig {
            algorithm: p1.alg.clone(),
            depth: p1.depth.unwrap_or(0),
            use_ml_eval: p1.use_ml_eval,
        },
    ];

    let num_games = cfg.times;
    let board_size = cfg.size;

    eprintln!("╔══════════════════════════════════════════╗");
    eprintln!("║       连锁棋 AI 对决基准测试             ║");
    eprintln!("╚══════════════════════════════════════════╝");
    eprintln!("  棋盘: {}×{}", board_size, board_size);
    eprintln!("  局数: {} (先手交替)", num_games);
    eprintln!("  AI 0: {}", name0);
    eprintln!("  AI 1: {}", name1);
    eprintln!("────────────────────────────────────────");

    let mut wins = [0u32; 2];

    let start = Instant::now();

    for game_id in 0..num_games {
        // 先手交替：game_id 为偶数时玩家 0 先手，奇数时玩家 1 先手
        let first = (game_id % 2) as usize;
        eprint!("  游戏 {}/{} ({}先手): ", game_id + 1, num_games,
            if first == 0 { &name0 } else { &name1 });

        let winner = play_game(board_size, 2, &ai_configs, first, game_id);

        match winner {
            Some(w) => {
                wins[w as usize] += 1;
                let w_name = if w == 0 { &name0 } else { &name1 };
                eprintln!("{} 胜", w_name);
            }
            None => eprintln!("平局"),
        }
    }

    let elapsed = start.elapsed();
    let avg_time = elapsed.as_secs_f64() / num_games as f64;
    let total = wins[0] + wins[1];
    let draws = num_games - total;

    eprintln!();
    eprintln!("═══════════ 测试结果 ═══════════");
    eprintln!("  {}: {} / {} ({:.0}%)", name0, wins[0], num_games,
        wins[0] as f64 / num_games as f64 * 100.0);
    eprintln!("  {}: {} / {} ({:.0}%)", name1, wins[1], num_games,
        wins[1] as f64 / num_games as f64 * 100.0);
    if draws > 0 {
        eprintln!("  平局: {}", draws);
    }
    eprintln!("  总耗时: {:.1}s, 平均 {:.1}s/局", elapsed.as_secs_f64(), avg_time);
    eprintln!();

    println!("RESULT: {}={} {}={} DRAW={} TOTAL={} TIME={:.1}s",
        name0, wins[0], name1, wins[1], draws, num_games, elapsed.as_secs_f64());
}
