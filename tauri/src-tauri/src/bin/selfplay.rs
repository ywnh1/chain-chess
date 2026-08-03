// 连锁棋 AI 自对弈数据生成器（训练数据）
// 推荐用法（混合随机：每局随机棋盘大小/玩家数/边界/阈值/AI 配置）:
//   cargo run --release --bin selfplay -- --mixed <num_games> <output> [seed]
//   例: cargo run --release --bin selfplay -- --mixed 5000 ./data/selfplay_mixed.jsonl 42
//
// 固定局面用法（兼容旧调用）:
//   cargo run --release --bin selfplay -- <board_size> <num_players> <num_games> <output> [border] [cap]
//   例: cargo run --release --bin selfplay -- 9 4 1000 ./data/selfplay.jsonl
//       cargo run --release --bin selfplay -- 9 4 1000 ./data/selfplay_wrap3.jsonl wrap 3
//
// border: default | wrap | bounce | degrade （默认 default）
// cap:    3 | 4 | 5 | random               （默认 4）

use std::env;

use chain_chess_lib::{BorderMode, CapMode, PlayerAiConfig, generate_selfplay_data, generate_selfplay_data_mixed};

fn print_usage(program: &str) {
    eprintln!("连锁棋 自对弈数据生成器 — 为 ML 模型训练收集对局数据");
    eprintln!("");
    eprintln!("用法:");
    eprintln!("  {0} --mixed <游戏局数> <输出路径> [seed]         # 每局随机局面（推荐）", program);
    eprintln!("  {0} <棋盘大小> <玩家数> <游戏局数> <输出路径> [border] [cap]  # 固定局面", program);
    eprintln!("");
    eprintln!("示例:");
    eprintln!("  {} --mixed 5000 ./data/selfplay_mixed.jsonl", program);
    eprintln!("  {} --mixed 5000 ./data/selfplay_mixed.jsonl 42", program);
    eprintln!("  {} 9 4 1000 ./data/selfplay_wrap3.jsonl wrap 3", program);
    eprintln!("");
    eprintln!("混合随机模式每局随机:");
    eprintln!("  棋盘大小 5~13  |  玩家数 2~4  |  边界 default/wrap/bounce/degrade  |  阈值 3/4/5");
    eprintln!("  AI 算法 alphabeta/pvs/strategy/mcts 与深度、ML 开关随机");
    eprintln!("  seed 固定时整批输出可复现；换 seed 得到不同批数据");
    eprintln!("");
    eprintln!("固定局面可选参数: border = default|wrap|bounce|degrade（默认 default）, cap = 3|4|5|random（默认 4）");
    eprintln!("输出: JSONL（每行一局的一步，含 board/borderMode/capMode/winner 等，直接供 train_xgb_model.py 训练）");
}

fn run_mixed(num_games: u32, output_path: &str, seed: u64) {
    eprintln!("┌────────────────────────────────────────────────────────────┐");
    eprintln!("│   连锁棋 混合随机自对弈数据生成器（ML 训练用）             │");
    eprintln!("└────────────────────────────────────────────────────────────┘");
    eprintln!("  局数 : {}", num_games);
    eprintln!("  输出 : {}", output_path);
    eprintln!("  seed : {}（固定可复现；换 seed 得到不同批数据）", seed);
    eprintln!("  模式 : 每局随机棋盘大小/玩家数/边界/阈值/AI 配置");
    eprintln!("");
    eprintln!("开始生成数据...");

    match generate_selfplay_data_mixed(num_games, output_path, seed) {
        Ok(_) => {
            eprintln!("");
            eprintln!("  👉 数据已就绪。训练: ./train.sh train");
        }
        Err(e) => {
            eprintln!("❌ 数据生成失败: {}", e);
            std::process::exit(1);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // ── 混合随机模式（推荐） ──
    if args.len() >= 4 && args[1] == "--mixed" {
        let num_games: u32 = args[2].parse().expect("游戏局数必须是数字");
        let output_path = &args[3];
        let seed: u64 = args.get(4).map(|s| s.parse().expect("seed 必须是数字")).unwrap_or(0);
        if let Some(parent) = std::path::Path::new(output_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        run_mixed(num_games, output_path, seed);
        return;
    }

    if args.len() < 5 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let board_size: usize = args[1].parse().expect("棋盘大小必须是数字 (5-19)");
    let num_players: usize = args[2].parse().expect("玩家数必须是数字 (2-10)");
    let num_games: u32 = args[3].parse().expect("游戏局数必须是数字");
    let output_path = &args[4];

    // 可选参数：边界模式与爆炸阈值模式（训练数据需覆盖任意模式）
    let border_mode = match args.get(5).map(|s| s.as_str()).unwrap_or("default") {
        "wrap" => BorderMode::Wrap,
        "bounce" => BorderMode::Bounce,
        "degrade" => BorderMode::Degrade,
        _ => BorderMode::Default,
    };
    let cap_mode = match args.get(6).map(|s| s.as_str()).unwrap_or("4") {
        "3" => CapMode::Cap3,
        "5" => CapMode::Cap5,
        "random" => CapMode::Random,
        _ => CapMode::Cap4,
    };

    if board_size < 5 || board_size > 19 {
        eprintln!("棋盘大小必须在 5~19 之间");
        std::process::exit(1);
    }
    if num_players < 2 || num_players > 10 {
        eprintln!("玩家数必须在 2~10 之间");
        std::process::exit(1);
    }

    eprintln!("┌────────────────────────────────────────────────────────────┐");
    eprintln!("│   连锁棋 固定局面自对弈数据生成器（ML 训练用）             │");
    eprintln!("└────────────────────────────────────────────────────────────┘");
    eprintln!("  棋盘 : {}×{}", board_size, board_size);
    eprintln!("  玩家 : {}", num_players);
    eprintln!("  局数 : {}", num_games);
    eprintln!("  输出 : {}", output_path);
    eprintln!("  边界 : {:?}", border_mode);
    eprintln!("  阈值 : {:?}", cap_mode);

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
            use_ml_eval: true,
        });
        eprintln!("  玩家 {}: {} depth={}", i, alg, depth);
    }

    // 创建输出目录
    if let Some(parent) = std::path::Path::new(output_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    eprintln!("");
    eprintln!("开始生成数据...");

    match generate_selfplay_data(board_size, num_players, num_games, &configs, output_path, border_mode, cap_mode) {
        Ok(_) => {
            eprintln!("");
            eprintln!("  👉 数据已就绪。训练: ./train.sh train");
        }
        Err(e) => {
            eprintln!("❌ 数据生成失败: {}", e);
            std::process::exit(1);
        }
    }
}
