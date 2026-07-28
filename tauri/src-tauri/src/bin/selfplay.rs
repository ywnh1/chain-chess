// 连锁棋 AI 自对弈数据生成器
// 用法: cargo run --release --bin selfplay -- <board_size> <num_players> <num_games> <output>
//
// 示例: cargo run --release --bin selfplay -- 9 4 1000 ./data/selfplay.jsonl

use std::env;

use chain_chess_lib::{PlayerAiConfig, generate_selfplay_data};

fn print_usage(program: &str) {
    eprintln!("用法: {} <棋盘大小> <玩家数> <游戏局数> <输出路径>", program);
    eprintln!("");
    eprintln!("示例:");
    eprintln!("  {} 9 4 5000 ./data/selfplay.jsonl", program);
    eprintln!("  {} 11 4 2000 ./data/selfplay_11x11.jsonl", program);
    eprintln!("");
    eprintln!("AI 配置（硬编码，可按需修改 src/bin/selfplay.rs）:");
    eprintln!("  玩家 0: Alpha-Beta depth=2");
    eprintln!("  玩家 1: 策略算法");
    eprintln!("  玩家 2: Alpha-Beta depth=3");
    eprintln!("  玩家 3: 策略算法");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 5 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let board_size: usize = args[1].parse().expect("棋盘大小必须是数字 (5-19)");
    let num_players: usize = args[2].parse().expect("玩家数必须是数字 (2-10)");
    let num_games: u32 = args[3].parse().expect("游戏局数必须是数字");
    let output_path = &args[4];

    if board_size < 5 || board_size > 19 {
        eprintln!("棋盘大小必须在 5~19 之间");
        std::process::exit(1);
    }
    if num_players < 2 || num_players > 10 {
        eprintln!("玩家数必须在 2~10 之间");
        std::process::exit(1);
    }

    eprintln!("╔══════════════════════════════════════════════════════╗");
    eprintln!("║       连锁棋 自对弈数据生成器                        ║");
    eprintln!("╚══════════════════════════════════════════════════════╝");
    eprintln!("  棋盘: {}×{}", board_size, board_size);
    eprintln!("  玩家: {}", num_players);
    eprintln!("  局数: {}", num_games);
    eprintln!("  输出: {}", output_path);

    // 构建 AI 配置（每个玩家一个）
    // 混合使用不同算法来增加训练数据的多样性
    let mut configs: Vec<PlayerAiConfig> = Vec::with_capacity(num_players);

    // 配置策略：玩家交替使用不同算法
    let alg_cycle = [
        ("alphabeta", 2),  // 快且有策略性
        ("strategy", 0),   // 纯启发式，增加多样性
        ("alphabeta", 3),  // 更深搜索，更强
        ("strategy", 0),   // 更多启发式多样性
        ("alphabeta", 2),
        ("strategy", 0),
        ("alphabeta", 1),  // 浅搜索快棋
        ("mcts", 1),       // MCTS 探索
        ("strategy", 0),
        ("alphabeta", 2),
    ];

    for i in 0..num_players {
        let (alg, depth) = alg_cycle[i % alg_cycle.len()];
        configs.push(PlayerAiConfig {
            algorithm: alg.to_string(),
            depth,
        });
        eprintln!("  玩家 {}: {} depth={}", i, alg, depth);
    }

    // 创建输出目录
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    eprintln!("");
    eprintln!("开始生成数据...");

    let start = std::time::Instant::now();
    match generate_selfplay_data(board_size, num_players, num_games, &configs, output_path) {
        Ok(steps) => {
            let elapsed = start.elapsed();
            let steps_per_sec = steps as f64 / elapsed.as_secs_f64();
            eprintln!("");
            eprintln!("✅ 数据生成完成!");
            eprintln!("   总步数: {}", steps);
            eprintln!("   耗时: {:.1}s", elapsed.as_secs_f64());
            eprintln!("   速度: {:.0} 步/秒", steps_per_sec);
            eprintln!("   文件: {}", output_path);
        }
        Err(e) => {
            eprintln!("❌ 数据生成失败: {}", e);
            std::process::exit(1);
        }
    }
}
