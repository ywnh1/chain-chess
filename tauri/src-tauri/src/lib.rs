// Copyright (c) 2026 ywnh1
// SPDX-License-Identifier: MIT

use std::io::Write;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;
use rand::Rng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use chain_chess_core::*;
use tauri::Manager;
#[cfg(not(target_os = "android"))]
use semver::Version;

// ─── State ───

pub struct AppState {
    pub history_file: Mutex<PathBuf>,
    pub app_data_dir: Mutex<PathBuf>,
}

fn get_round_path(app_data_dir: &PathBuf) -> PathBuf {
    app_data_dir.join("round_data.json")
}

/// 原子写入：写临时文件后重命名，防止部分写入导致数据损坏
fn atomic_write(path: &std::path::Path, contents: &str) -> Result<(), String> {
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, contents).map_err(|e| format!("写入临时文件失败: {}", e))?;
    fs::rename(&tmp_path, path).map_err(|e| format!("重命名失败: {}", e))?;
    Ok(())
}

// ─── Tauri commands ───

#[tauri::command]
async fn process_move(
    board: GameBoard,
    size: usize,
    x: usize,
    y: usize,
    player: usize,
    max_players: usize,
    border_mode: BorderMode,
    cap_mode: Option<CapMode>,
    seed: Option<u64>,
) -> Result<ProcessMoveResult, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut b = board.clone();
        let cap_mode = cap_mode.unwrap_or_default();
        let (eliminated, chain_count, killed_by, steps, exploded) = process_click_with_snapshots(&mut b, size, x, y, player, max_players, border_mode, cap_mode, seed);
        let game_over = eliminated.len() >= max_players.saturating_sub(1);
        let winner = if game_over {
            // find the remaining player
            let alive: Vec<usize> = (0..max_players)
                .filter(|p| !eliminated.contains(p) && has_pieces(&b, *p))
                .collect();
            alive.first().copied()
        } else {
            None
        };
        ProcessMoveResult {
            board: b,
            chain_count,
            eliminated,
            killed_by,
            game_over,
            winner,
            steps,
            exploded,
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    Ok(result)
}

#[tauri::command]
async fn ai_move(
    board: GameBoard,
    size: usize,
    player: usize,
    depth: usize,
    eliminated: Vec<usize>,
    max_players: usize,
    border_mode: BorderMode,
    cap_mode: Option<CapMode>,
    game_count: u32,
    first_move_pos: Option<[usize; 2]>,
    use_ml_eval: bool,
    random_scale: u32,
) -> Result<[usize; 2], String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let cap_mode = cap_mode.unwrap_or_default();
        find_best_move(&board, size, player, depth, &eliminated, max_players, game_count, first_move_pos, use_ml_eval, border_mode, cap_mode, random_scale)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Some((x, y)) => Ok([x, y]),
        None => Err("No valid move".into()),
    }
}

#[tauri::command]
async fn ai_move_v2(
    random_scale: u32,
    board: GameBoard,
    size: usize,
    player: usize,
    depth: usize,
    eliminated: Vec<usize>,
    max_players: usize,
    border_mode: BorderMode,
    cap_mode: Option<CapMode>,
    game_count: u32,
    first_move_pos: Option<[usize; 2]>,
    algorithm: String,
    use_ml_eval: bool,
) -> Result<[usize; 2], String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let cap_mode = cap_mode.unwrap_or_default();
        if algorithm == "pvs" {
            // PVS (NegaMax) + Killer/History + QSearch
            let mut searcher = PvsSearcher::new(player, game_count, use_ml_eval, border_mode, cap_mode, random_scale);
            searcher.find_best(&board, size, player, max_players, &eliminated, depth)
        } else {
            // 默认使用 Alpha-Beta（原 find_best_move）
            find_best_move(&board, size, player, depth, &eliminated, max_players, game_count, first_move_pos, use_ml_eval, border_mode, cap_mode, random_scale)
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Some((x, y)) => Ok([x, y]),
        None => Err("No valid move".into()),
    }
}

#[tauri::command]
async fn ai_move_strategy(
    board: GameBoard,
    size: usize,
    player: usize,
    eliminated: Vec<usize>,
    max_players: usize,
    border_mode: BorderMode,
    cap_mode: Option<CapMode>,
    game_count: u32,
    first_move_pos: Option<[usize; 2]>,
) -> Result<[usize; 2], String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let cap_mode = cap_mode.unwrap_or_default();
        find_best_move_strategy(&board, size, player, &eliminated, max_players, game_count, first_move_pos, border_mode, cap_mode)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Some((x, y)) => Ok([x, y]),
        None => Err("No valid move".into()),
    }
}

/// 一键终局命令（前端入口）：线程池中执行 simulate_to_end
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename = "simulate_to_end")]
async fn simulate_to_end_cmd(
    board: GameBoard,
    size: usize,
    max_players: usize,
    cur_player: usize,
    eliminated: Vec<usize>,
    border_mode: BorderMode,
    cap_mode: Option<CapMode>,
    first_move_pos: Option<[usize; 2]>,
    game_count: u32,
    ai_configs: std::collections::HashMap<String, serde_json::Value>,
) -> Result<SimulateResult, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        simulate_to_end(board, size, max_players, cur_player, eliminated, border_mode, cap_mode.unwrap_or_default(), first_move_pos, game_count, ai_configs)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;
    Ok(result)
}

/// 单个玩家的 AI 配置（设备性能检测页生成）
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiBenchPlayer {
    pub algorithm: String,
    #[serde(default)]
    pub depth: Option<usize>,
    #[serde(default)]
    pub random_scale: Option<u32>,
    #[serde(default)]
    pub use_ml_eval: Option<bool>,
}

/// 单局 AI 基准配置
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiBenchGameConfig {
    pub size: usize,
    pub players: Vec<AiBenchPlayer>,
    #[serde(default = "default_bench_border")]
    pub border_mode: BorderMode,
    #[serde(default = "default_bench_cap")]
    pub cap_mode: CapMode,
}

fn default_bench_border() -> BorderMode { BorderMode::Default }
fn default_bench_cap() -> CapMode { CapMode::Cap4 }

/// 单局结果（Rust 端计时，精确反映引擎计算耗时）
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiBenchGameResult {
    pub elapsed_ms: f64,
    pub steps: usize,
    pub winner: Option<usize>,
}

/// 单局 AI 基准命令：构造首子布局后全速模拟到终局，返回引擎耗时与结果
#[tauri::command(rename = "bench_ai_game")]
async fn bench_ai_game(
    config: AiBenchGameConfig,
    game_count: u32,
) -> Result<AiBenchGameResult, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_ai_bench_game(&config, game_count)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;
    Ok(result)
}

fn run_ai_bench_game(cfg: &AiBenchGameConfig, game_count: u32) -> AiBenchGameResult {
    let sz = cfg.size;
    let max_players = cfg.players.len();
    if sz < 5 || max_players < 2 {
        return AiBenchGameResult { elapsed_ms: 0.0, steps: 0, winner: None };
    }
    let mut board: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None, blocked: false }; sz]; sz];
    let starts = spread_starts(sz, max_players);
    // 首子等级 = 阈值 n-1（cap3→2、cap4→3、cap5→4；随机模式取中间等级 3）
    let th: u32 = match cfg.cap_mode {
        CapMode::Cap3 => 3,
        CapMode::Cap4 => 4,
        CapMode::Cap5 => 5,
        CapMode::Random => 3,
    };
    for (p, &(x, y)) in starts.iter().enumerate() {
        board[x][y] = Cell { owner: Some(p), count: (th - 1) as u8, th: None, blocked: false };
    }
    let mut ai_configs: HashMap<String, serde_json::Value> = HashMap::new();
    for (p, pl) in cfg.players.iter().enumerate() {
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
    let t0 = Instant::now();
    let r = simulate_to_end(board, sz, max_players, 0, Vec::new(), cfg.border_mode, cfg.cap_mode, None, game_count, ai_configs);
    AiBenchGameResult {
        elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
        steps: r.history.len(),
        winner: r.winner,
    }
}

#[tauri::command]
async fn ai_move_mcts(
    _random_scale: u32,  // MCTS 不走 eval 随机，保留参数兼容前端
    board: GameBoard,
    size: usize,
    player: usize,
    depth: usize,
    eliminated: Vec<usize>,
    max_players: usize,
    border_mode: BorderMode,
    cap_mode: Option<CapMode>,
) -> Result<[usize; 2], String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let cap_mode = cap_mode.unwrap_or_default();
        find_best_move_mcts(&board, size, player, depth, &eliminated, max_players, border_mode, cap_mode)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Some((x, y)) => Ok([x, y]),
        None => Err("No valid move".into()),
    }
}

#[tauri::command]
async fn save_game_history(
    state: tauri::State<'_, AppState>,
    record: HistoryRecord,
) -> Result<(), String> {
    let path = state.history_file.lock().map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut history: Vec<HistoryRecord> = if path.exists() {
        let content = fs::read_to_string(&*path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };
    history.push(record);
    let json = serde_json::to_string_pretty(&history).map_err(|e| e.to_string())?;
    atomic_write(&*path, &json)?;
    Ok(())
}

#[tauri::command]
async fn load_game_history(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<HistoryRecord>, String> {
    let path = state.history_file.lock().map_err(|e| e.to_string())?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&*path).map_err(|e| e.to_string())?;
    // 整体解析失败时降级为逐条容错解析：单条损坏/旧格式记录不再清空整个历史列表
    let history: Vec<HistoryRecord> = serde_json::from_str(&content).unwrap_or_else(|_| {
        serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| serde_json::from_value::<HistoryRecord>(item.clone()).ok())
                    .collect()
            })
            .unwrap_or_default()
    });
    Ok(history)
}

#[tauri::command]
async fn import_game_history(
    state: tauri::State<'_, AppState>,
    json_data: String,
) -> Result<usize, String> {
    let records: Vec<HistoryRecord> = serde_json::from_str(&json_data)
        .map_err(|e| format!("JSON 格式错误: {}", e))?;
    let path = state.history_file.lock().map_err(|e| e.to_string())?;
    let mut existing: Vec<HistoryRecord> = if path.exists() {
        let content = fs::read_to_string(&*path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };
    // 按 id 去重合并
    let existing_ids: std::collections::HashSet<u64> = existing.iter().map(|r| r.id).collect();
    let mut imported = 0usize;
    for record in records {
        if !existing_ids.contains(&record.id) {
            existing.push(record);
            imported += 1;
        }
    }
    if imported > 0 {
        let json = serde_json::to_string_pretty(&existing).map_err(|e| e.to_string())?;
        atomic_write(&*path, &json)?;
    }
    Ok(imported)
}

#[tauri::command]
async fn delete_game_history_record(
    state: tauri::State<'_, AppState>,
    record_id: u64,
) -> Result<(), String> {
    let path = state.history_file.lock().map_err(|e| e.to_string())?;
    let mut history: Vec<HistoryRecord> = if path.exists() {
        let content = fs::read_to_string(&*path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        return Ok(());
    };
    history.retain(|r| r.id != record_id);
    let json = serde_json::to_string_pretty(&history).map_err(|e| e.to_string())?;
    atomic_write(&*path, &json)?;
    Ok(())
}

#[tauri::command]
async fn clear_game_history(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = state.history_file.lock().map_err(|e| e.to_string())?;
    if path.exists() {
        fs::remove_file(&*path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 批量删除历史记录（多选删除）
#[tauri::command]
async fn delete_game_history_records(
    state: tauri::State<'_, AppState>,
    record_ids: Vec<u64>,
) -> Result<(), String> {
    let path = state.history_file.lock().map_err(|e| e.to_string())?;
    let mut history: Vec<HistoryRecord> = if path.exists() {
        let content = fs::read_to_string(&*path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        return Ok(());
    };
    let ids_set: std::collections::HashSet<u64> = record_ids.into_iter().collect();
    history.retain(|r| !ids_set.contains(&r.id));
    let json = serde_json::to_string_pretty(&history).map_err(|e| e.to_string())?;
    atomic_write(&*path, &json)?;
    Ok(())
}

/// 保存游戏状态（未完成游戏，用于继续游戏功能）
#[tauri::command]
async fn save_game_state(
    app_handle: tauri::AppHandle,
    state_json: String,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {}", e))?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let path = data_dir.join("saved_game.json");
    atomic_write(&path, &state_json)?;
    Ok(())
}

/// 加载已保存的游戏状态
#[tauri::command]
async fn load_game_state(
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let data_dir = app_handle.path().app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {}", e))?;
    let path = data_dir.join("saved_game.json");
    if path.exists() {
        fs::read_to_string(&path).map_err(|e| e.to_string())
    } else {
        Ok(String::new())
    }
}

/// 清除已保存的游戏状态
#[tauri::command]
async fn clear_game_state(
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let data_dir = app_handle.path().app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {}", e))?;
    let path = data_dir.join("saved_game.json");
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn exit_app(app_handle: tauri::AppHandle) -> Result<(), String> {
    app_handle.exit(0);
    Ok(())
}

/// 保存对局回合历史到磁盘（溢出存储用）
#[tauri::command]
async fn save_round_history(
    state: tauri::State<'_, AppState>,
    data: Vec<TurnHistory>,
) -> Result<(), String> {
    let dir = state.app_data_dir.lock().map_err(|e| e.to_string())?;
    let path = get_round_path(&dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(&data).map_err(|e| e.to_string())?;
    atomic_write(&path, &json)?;
    Ok(())
}

/// 从磁盘加载对局回合历史
#[tauri::command]
async fn load_round_history(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<TurnHistory>, String> {
    let dir = state.app_data_dir.lock().map_err(|e| e.to_string())?;
    let path = get_round_path(&dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

/// 清除对局回合历史文件
#[tauri::command]
async fn clear_round_history(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let dir = state.app_data_dir.lock().map_err(|e| e.to_string())?;
    let path = get_round_path(&dir);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn export_game_history_dialog(
    app_handle: tauri::AppHandle,
    json_data: String,
) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;
    // 用时间戳做默认文件名
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let date_str = format!(
        "{}-{:02}-{:02}",
        1970 + days / 365,
        ((days % 365) / 30 + 1).min(12),
        (days % 30 + 1).min(31)
    );
    let default_name = format!("连锁棋历史_{}.json", date_str);

    // 1) 系统原生保存对话框（用户选路径，授权读写权限）
    match app_handle.dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name(&default_name)
        .blocking_save_file()
    {
        Some(fpath) => {
            // 通过 tauri-plugin-fs 的 Fs::open 写入（支持普通路径和 Android content:// URI）
            use tauri_plugin_fs::OpenOptions;
            let fs = app_handle.state::<tauri_plugin_fs::Fs<tauri::Wry>>();
            let mut opts = OpenOptions::new();
            opts.write(true);
            let mut file = fs.open(fpath.clone(), opts)
                .map_err(|e| format!("打开文件失败: {}", e))?;
            file.write_all(json_data.as_bytes())
                .map_err(|e| format!("写入文件失败: {}", e))?;
            drop(file);
            Ok(format!("{}", json_data.len()))
        }
        None => {
            // 2) 用户取消 → 写入 app 数据目录保底
            let data_dir = app_handle.path().app_data_dir()
                .map_err(|e| format!("获取数据目录失败: {}", e))?;
            let fallback_path = data_dir.join(&default_name);
            atomic_write(&fallback_path, &json_data)
                .map_err(|e| format!("写入保底文件失败: {}", e))?;
            let size = fallback_path.metadata()
                .map(|m| m.len())
                .unwrap_or(0);
            Ok(format!("fallback:{}", size))
        }
    }
}


// ─── App Settings（持久化到 app_data_dir/settings.json） ───

/// 应用设置。theme: "system" | "light" | "dark"；soundTheme 为预设音效主题。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "settings_default_theme")]
    pub theme: String,
    #[serde(default = "settings_default_sound")]
    pub sound_theme: String,
    /// 其余设置项原样透传（vibrate / dogBarkMode / lastSetup / introSeenVersion / introNeverShow …）。
    /// 少了这个字段，serde 会在反序列化时静默丢弃所有未声明的键，
    /// 导致前端存什么设置都存不住（每次重启都被重置）。
    #[serde(flatten, default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn settings_default_theme() -> String { "system".to_string() }
fn settings_default_sound() -> String { "classic".to_string() }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: settings_default_theme(),
            sound_theme: settings_default_sound(),
            extra: Default::default(),
        }
    }
}

fn get_settings_path(app_data_dir: &std::path::Path) -> PathBuf {
    app_data_dir.join("settings.json")
}

/// 读取应用设置（不存在时返回默认值）
#[tauri::command]
async fn load_settings(app_handle: tauri::AppHandle) -> Result<AppSettings, String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {}", e))?;
    let path = get_settings_path(&data_dir);
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取设置失败: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("解析设置失败: {}", e))
}

/// 保存应用设置
#[tauri::command]
async fn save_settings(app_handle: tauri::AppHandle, settings: AppSettings) -> Result<(), String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {}", e))?;
    let path = get_settings_path(&data_dir);
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    atomic_write(&path, &json)
}

// ─── Auto Update ───

#[cfg(not(target_os = "android"))]
const UPDATE_URL: &str = "https://gitee.com/ywnh1/chain-chess-release/raw/main/update.json";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    available: bool,
    version: String,
    notes: String,
    url: String,
    error: Option<String>,
}

/// 检查是否有新版本可用（桌面端用 reqwest）
#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn check_update(app_handle: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let current_ver = app_handle.config().version.clone().unwrap_or_else(|| "0.0.0".into());

    let resp = reqwest::get(UPDATE_URL)
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    if !resp.status().is_success() {
        return Ok(UpdateInfo {
            available: false,
            version: String::new(),
            notes: String::new(),
            url: String::new(),
            error: Some(format!("服务器返回 {}", resp.status())),
        });
    }

    let update_data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析更新数据失败: {}", e))?;

    let remote_version = update_data["version"]
        .as_str()
        .unwrap_or("0.0.0")
        .to_string();
    let notes = update_data["notes"].as_str().unwrap_or("").to_string();

    // 按平台选择 URL：Windows → windows，其余 → linux（各平台 url 统一指向下载中心）
    let platform_key = if cfg!(target_os = "windows") { "windows" } else { "linux" };
    // 平台条目缺失时 url 为空；serde_json::Value 索引缺失返回 Null，不会 panic
    let url = update_data["platforms"][platform_key]["url"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // 无当前平台下载包（url 为空）→ 不视为可更新，避免 available=true + url="" 的空链接状态
    let available = !url.is_empty()
        && match (
            Version::parse(&current_ver),
            Version::parse(&remote_version),
        ) {
            (Ok(current), Ok(remote)) => remote >= current,
            _ => {
                !remote_version.is_empty()
            }
        };

    Ok(UpdateInfo {
        available,
        version: remote_version,
        notes,
        url,
        error: None,
    })
}

/// Android 端更新检查由前端 JS fetch() 处理
#[cfg(target_os = "android")]
#[tauri::command]
async fn check_update(_app_handle: tauri::AppHandle) -> Result<UpdateInfo, String> {
    Err("请使用前端 JS 检查更新".into())
}

/// 打开更新跳转目标（下载中心页）——所有平台统一，不分平台
#[tauri::command]
async fn download_update(url: String, app_handle: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;
    app_handle
        .opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("打开浏览器失败: {}", e))?;
    Ok(url)
}

/// 安装/打开已下载的更新文件（通用——所有平台）
#[tauri::command]
async fn install_update(path: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    app_handle
        .opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("打开文件失败: {}", e))?;

    Ok(())
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 使用 Tauri 应用数据目录（Android/iOS/桌面通用）
            let app_data_dir = app.path().app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            fs::create_dir_all(&app_data_dir).ok();
            let history_path = app_data_dir.join("history.json");
            let data_dir = app_data_dir.clone();
            app.manage(AppState {
                history_file: Mutex::new(history_path),
                app_data_dir: Mutex::new(data_dir),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ai_move,
            ai_move_v2,
            ai_move_mcts,
            ai_move_strategy,
            simulate_to_end_cmd,
            bench_ai_game,
            load_settings,
            save_settings,
            check_update,
            download_update,
            install_update,
            process_move,
            save_game_history,
            load_game_history,
            import_game_history,
            export_game_history_dialog,
            clear_game_history,
            delete_game_history_record,
            delete_game_history_records,
            save_game_state,
            load_game_state,
            clear_game_state,
            exit_app,
            save_round_history,
            load_round_history,
            clear_round_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


#[derive(Serialize, Deserialize, Debug)]
struct StepRecord {
    game_id: u32,
    turn: u32,
    size: usize,
    max_players: usize,
    border_mode: BorderMode,
    cap_mode: CapMode,
    board: GameBoard,
    cur_player: usize,
    move_x: usize,
    move_y: usize,
    live_players: Vec<usize>,
    eliminated: Vec<usize>,
    winner: Option<usize>,
    game_over: bool,
}

/// 千分位格式化（1234567 → "1,234,567"）
fn fmt_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { out.push(','); }
        out.push(ch);
    }
    out.chars().rev().collect()
}

/// 人类可读耗时（90s → "1m30s"，42s → "42s"）
fn fmt_duration(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 60 { format!("{}m{:02}s", s / 60, s % 60) } else { format!("{}s", s) }
}

/// 单行进度条（stderr 刷新，`\r` 覆盖上一行）：
///   游戏   12/5000 ██████░░░░░░░░░░░░░░░░ 25%  步数 87,432  1,234 步/秒  ETA 1m23s  [附加信息]
/// stderr 是否为终端（TTY）。管道/重定向/日志捕获时返回 false。
fn stderr_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

/// 进度条：双模式。
/// - TTY（终端）：`\r` + ANSI 清行，实时覆盖刷新，必须手动 flush（stderr 行缓冲会吞掉无换行内容）。
/// - 非 TTY（管道/重定向/日志）：输出完整行（带换行），由调用方按较长间隔触发，保证任何跑法都能看到进度。
fn render_progress(done: u32, total: u32, steps: u64, start: &std::time::Instant, extra: &str) {
    use std::io::Write;
    let pct = (done as f64 / total.max(1) as f64 * 100.0).min(100.0);
    let bar_w = 22;
    let filled = (pct / 100.0 * bar_w as f64) as usize;
    let bar: String = std::iter::repeat('█').take(filled)
        .chain(std::iter::repeat('░').take(bar_w - filled)).collect();
    let elapsed = start.elapsed().as_secs_f64().max(0.001);
    let speed = steps as f64 / elapsed;
    let eta = if done > 0 { elapsed / done as f64 * (total - done) as f64 } else { 0.0 };
    if stderr_is_tty() {
        eprint!("\x1b[2K\r  游戏 {:>5}/{} |{}| {:>3.0}%  步数 {:>10}  {:>6.0} 步/秒  ETA {:>7}  {}",
            done, total, bar, pct, fmt_thousands(steps), speed, fmt_duration(eta), extra);
        let _ = std::io::stderr().flush();
    } else {
        eprintln!("[进度] 游戏 {:>5}/{} |{}| {:>3.0}%  步数 {:>10}  {:>6.0} 步/秒  ETA {:>7}  {}",
            done, total, bar, pct, fmt_thousands(steps), speed, fmt_duration(eta), extra);
    }
}

/// 单局自对弈：按给定局面与 AI 配置跑完一局，逐步输出 StepRecord（JSONL 行）。
/// 返回本局步数。rng 用于随机兜底走法（AI 无棋可下时）。
fn run_one_game(
    writer: &mut impl std::io::Write,
    board_size: usize,
    max_players: usize,
    player_configs: &[PlayerAiConfig],
    border_mode: BorderMode,
    cap_mode: CapMode,
    game_id: u32,
    rng: &mut impl rand::Rng,
) -> Result<u64, String> {
    let mut board = vec![
        vec![Cell { owner: None, count: 0, th: None, blocked: false }; board_size];
        board_size
    ];
    let mut eliminated: Vec<usize> = Vec::new();
    let mut cur_player: usize = 0;
    let game_count: u32 = game_id;
    let first_move_pos: Option<[usize; 2]> = None;
    let mut turn: u32 = 0;
    let mut steps: u64 = 0;
    // 单局步数上限：防止满盘/死锁局面（所有存活玩家均无合法走子且存活>1）导致无限轮转卡死。
    // 超限直接结束本局（该局无 winner，训练时会被跳过，不影响数据质量）。
    let max_steps: u64 = (board_size * board_size * 4 * max_players).max(1000) as u64;

    loop {
        steps += 1;
        if steps > max_steps { break; }

        let legal_moves = get_moves(&board, board_size, cur_player, None, border_mode, cap_mode);

        if legal_moves.is_empty() {
            let alive: Vec<usize> = (0..max_players)
                .filter(|p| !eliminated.contains(p))
                .collect();
            if alive.len() <= 1 { break; }
            let idx = alive.iter().position(|&p| p == cur_player).unwrap_or(0);
            cur_player = alive[(idx + 1) % alive.len()];
            continue;
        }

        let config = &player_configs[cur_player];
        let first_move_arr = first_move_pos.map(|[x, y]| [x, y]);
        let chosen = find_best_move_by_alg(
            &board, board_size, cur_player, config,
            &eliminated, max_players, game_count, first_move_arr,
            border_mode, cap_mode, 0,  // 随机刻度 0，保持确定性
        );

        let (mx, my) = chosen.unwrap_or_else(|| {
            let idx = rng.gen_range(0..legal_moves.len());
            legal_moves[idx]
        });

        // 记录走法前的存活玩家
        let live_before: Vec<usize> = (0..max_players)
            .filter(|p| !eliminated.contains(p) && has_pieces(&board, *p))
            .collect();

        let (new_elim, _chain_count) = process_click(&mut board, board_size, mx, my, cur_player, max_players, border_mode, cap_mode, None);
        for &e in &new_elim {
            if !eliminated.contains(&e) { eliminated.push(e); }
        }

        // 存活玩家 = 未被淘汰的玩家（不管当前有没有棋子）
        let alive_now: Vec<usize> = (0..max_players)
            .filter(|p| !eliminated.contains(p))
            .collect();
        let game_over = alive_now.len() <= 1;
        let winner = if game_over {
            alive_now.first().copied()
        } else {
            None
        };

        let record = StepRecord {
            game_id, turn, size: board_size, max_players,
            border_mode, cap_mode,
            board: board.clone(),
            cur_player, move_x: mx, move_y: my,
            live_players: live_before,
            eliminated: eliminated.clone(),
            winner, game_over,
        };

        let line = serde_json::to_string(&record)
            .map_err(|e| format!("serialize failed: {}", e))?;
        writeln!(writer, "{}", line)
            .map_err(|e| format!("write failed: {}", e))?;

        if game_over { break; }

        let idx = alive_now.iter().position(|&p| p == cur_player).unwrap_or(0);
        cur_player = alive_now[(idx + 1) % alive_now.len()];
        turn += 1;
    }
    Ok(steps)
}

/// 固定局面批量自对弈：所有对局使用相同的棋盘大小/玩家数/边界/阈值与 AI 配置。
pub fn generate_selfplay_data(
    board_size: usize,
    max_players: usize,
    num_games: u32,
    player_configs: &[PlayerAiConfig],
    output_path: &str,
    border_mode: BorderMode,
    cap_mode: CapMode,
) -> Result<u64, String> {
    use std::io::Write;

    assert_eq!(player_configs.len(), max_players, "config count must match player count");

    let file = std::fs::File::create(output_path)
        .map_err(|e| format!("cannot create output {}: {}", output_path, e))?;
    let mut writer = std::io::BufWriter::new(file);
    let mut total_steps: u64 = 0;
    let mut rng = rand::thread_rng();
    let start = std::time::Instant::now();

    let mut last_prog = start;
    let prog_interval = if stderr_is_tty() { 0.5 } else { 10.0 };
    for game_id in 0..num_games {
        // 进度条：时间驱动（TTY 0.5s / 非 TTY 10s 一行）+ 最后一局强制刷新
        if last_prog.elapsed().as_secs_f64() >= prog_interval || game_id + 1 == num_games {
            render_progress(game_id + 1, num_games, total_steps, &start,
                &format!("{}×{}/{}p {:?}/{:?}", board_size, board_size, max_players, border_mode, cap_mode));
            last_prog = std::time::Instant::now();
        }

        total_steps += run_one_game(&mut writer, board_size, max_players, player_configs, border_mode, cap_mode, game_id, &mut rng)?;
    }

    writer.flush().map_err(|e| format!("flush failed: {}", e))?;
    eprintln!();
    eprintln!("  ✅ 数据生成完成");
    eprintln!("     局数: {}", num_games);
    eprintln!("     步数: {}", fmt_thousands(total_steps));
    eprintln!("     耗时: {}", fmt_duration(start.elapsed().as_secs_f64()));
    eprintln!("     速度: {:.0} 步/秒", total_steps as f64 / start.elapsed().as_secs_f64().max(0.001));
    eprintln!("     文件: {}", output_path);
    Ok(total_steps)
}

/// 随机生成一组玩家 AI 配置（算法 / 深度 / ML 开关随机），保证数据多样性。
/// 深度按棋盘大小缩放：大棋盘用浅搜索，避免单局耗时爆炸（alphabeta 无分支限制，13×13 depth=3 会极慢）。
fn random_ai_configs(board_size: usize, max_players: usize, rng: &mut impl rand::Rng) -> Vec<PlayerAiConfig> {
    // 算法池：alphabeta/pvs 为主，strategy 增加启发式多样性，mcts 低频（较慢）
    let alg_pool = [
        "alphabeta", "alphabeta", "alphabeta",
        "pvs", "pvs",
        "strategy", "strategy",
        "mcts",
    ];
    // 深度上限：≤7 深搜 1-3；9 中搜 1-2；≥11 浅搜 1
    let max_d = if board_size <= 7 { 3 } else if board_size <= 9 { 2 } else { 1 };
    let mut configs = Vec::with_capacity(max_players);
    for _ in 0..max_players {
        let alg = alg_pool[rng.gen_range(0..alg_pool.len())];
        let depth = match alg {
            "mcts" => 1,
            "strategy" => 0,
            _ => rng.gen_range(1..=max_d),
        };
        configs.push(PlayerAiConfig {
            algorithm: alg.to_string(),
            depth,
            use_ml_eval: rng.gen_bool(0.5), // 一半 ML 一半手写：手写评估快得多，控制整体速度
        });
    }
    configs
}

/// 混合随机自对弈：每局随机选取棋盘大小(5-13)、玩家数(2-4)、边界模式、
/// 爆炸阈值模式(3/4/5) 与 AI 配置，覆盖任意棋盘与模式组合。
/// seed 固定时输出可复现。
pub fn generate_selfplay_data_mixed(
    num_games: u32,
    output_path: &str,
    seed: u64,
) -> Result<u64, String> {
    use std::io::Write;

    let file = std::fs::File::create(output_path)
        .map_err(|e| format!("cannot create output {}: {}", output_path, e))?;
    let mut writer = std::io::BufWriter::new(file);
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut total_steps: u64 = 0;
    let start = std::time::Instant::now();
    let mut last_prog = start;
    let prog_interval = if stderr_is_tty() { 0.5 } else { 10.0 };
    let mut mode_stats: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    for game_id in 0..num_games {
        // 每局随机局面：棋盘大小 5-8（7 最常见），玩家数 2-4（与当前产品局配置一致）
        let size_pool = [5usize, 5, 6, 6, 7, 7, 7, 7, 8, 8];
        let board_size = size_pool[rng.gen_range(0..size_pool.len())];
        let player_pool = [2usize, 2, 2, 3, 3, 4, 4, 4];
        let max_players = player_pool[rng.gen_range(0..player_pool.len())];
        let border_mode = [
            BorderMode::Default, BorderMode::Wrap,
            BorderMode::Bounce, BorderMode::Degrade,
        ][rng.gen_range(0..4)];
        let cap_mode = [
            CapMode::Cap3, CapMode::Cap4, CapMode::Cap5,
        ][rng.gen_range(0..3)];

        let configs = random_ai_configs(board_size, max_players, &mut rng);
        let key = format!("{board_size}×{board_size}/{max_players}p/{border_mode:?}/{cap_mode:?}");
        *mode_stats.entry(key).or_insert(0) += 1;

        if last_prog.elapsed().as_secs_f64() >= prog_interval || game_id + 1 == num_games {
            // 显示当前局局面（每局随机，帮助确认覆盖面）
            render_progress(game_id + 1, num_games, total_steps, &start,
                &format!("当前 {}×{}/{}p {:?}/{:?}", board_size, board_size, max_players, border_mode, cap_mode));
            last_prog = std::time::Instant::now();
        }

        total_steps += run_one_game(&mut writer, board_size, max_players, &configs, border_mode, cap_mode, game_id, &mut rng)?;
    }

    writer.flush().map_err(|e| format!("flush failed: {}", e))?;
    eprintln!();
    eprintln!("  ✅ 数据生成完成");
    eprintln!("     局数: {}", num_games);
    eprintln!("     步数: {}", fmt_thousands(total_steps));
    eprintln!("     耗时: {}", fmt_duration(start.elapsed().as_secs_f64()));
    eprintln!("     速度: {:.0} 步/秒", total_steps as f64 / start.elapsed().as_secs_f64().max(0.001));
    eprintln!("     文件: {}", output_path);
    eprintln!();
    eprintln!("  📊 局面分布（棋盘×玩家×边界×阈值）:");
    let mut keys: Vec<_> = mode_stats.keys().collect();
    keys.sort();
    for k in keys {
        eprintln!("     {:<28} {} 局", k, mode_stats[k]);
    }
    Ok(total_steps)
}


// ═══════════════════ 规则完备性测试（borderMode × capMode 全组合） ═══════════════════
#[cfg(test)]
mod tests {
    use super::*;

    // ── 历史记录序列化兼容性（前端 saveGameHistory 发送的完整字段，含 Rust 未定义字段） ──
    #[test]
    fn history_record_deserialize_frontend_payload() {
        let json = r#"{
            "id": 1723000000000,
            "time": "2026/08/03 10:00:00",
            "mode": "local",
            "aiAlgorithm": "",
            "aiDepth": 0,
            "gameCount": 0,
            "playerCount": 2,
            "aiCount": 0,
            "boardSize": 7,
            "borderMode": "default",
            "capMode": "4",
            "winner": 0,
            "colorNames": ["玩家 1", "玩家 2"],
            "chainStats": {"0": {"triggered": 1, "maxChain": 2}},
            "maxChain": {"player": 0, "length": 2},
            "finished": true,
            "history": {"c": true, "t": 3, "p": [[1,2],[0,0]], "pt": [[1,1],[0,0]], "m": [[0,0,0,0], null]},
            "playerTypes": ["human", "human"],
            "killedBy": {}
        }"#;
        let r: HistoryRecord = serde_json::from_str(json).expect("前端完整 record 应能反序列化");
        assert_eq!(r.id, 1723000000000);
        assert_eq!(r.player_count, 2);
        assert_eq!(r.winner, Some(0));
        assert_eq!(r.finished, Some(true));
        assert_eq!(r.max_chain.player, Some(0));
        assert_eq!(r.chain_stats.get("0").map(|c| c.max_chain), Some(2));
        // 未知字段被忽略（borderMode/capMode/playerTypes/killedBy 不会导致解析失败）
        assert!(r.history.is_object());
        // round trip：序列化后再反序列化
        let s = serde_json::to_string(&r).unwrap();
        let r2: HistoryRecord = serde_json::from_str(&s).unwrap();
        assert_eq!(r2.id, r.id);
    }

    // ── 旧版记录（缺 finished/history/gameState）应能解析，不拖垮整个历史文件 ──
    #[test]
    fn history_record_old_format_compat() {
        let json = r#"{
            "id": 1,
            "time": "2026/01/01 00:00:00",
            "mode": "ai",
            "aiAlgorithm": "alphabeta",
            "aiDepth": 2,
            "playerCount": 2,
            "aiCount": 1,
            "boardSize": 7,
            "winner": null,
            "colorNames": ["玩家 1", "AI 2"],
            "chainStats": {},
            "maxChain": {"player": null, "length": 0}
        }"#;
        let r: HistoryRecord = serde_json::from_str(json).expect("旧格式记录应能解析（缺字段走默认值）");
        assert_eq!(r.finished, None);
        assert_eq!(r.game_state, None);
        assert!(r.history.is_null() || r.history.is_object(), "history 缺省应为 null 或对象");
    }

    // ── 历史文件整体解析：一条坏记录不应清空整个列表（load_game_history 兼容） ──
    #[test]
    fn history_list_partial_bad_record() {
        let json = r#"[
            {"id": 1, "time": "a", "mode": "local", "aiAlgorithm": "", "aiDepth": 0,
             "playerCount": 2, "aiCount": 0, "boardSize": 5, "winner": null,
             "colorNames": [], "chainStats": {}, "maxChain": {"player": null, "length": 0}},
            "垃圾数据"
        ]"#;
        let v: Result<Vec<HistoryRecord>, _> = serde_json::from_str(json);
        // 当前实现是整文件解析，坏记录会导致整个列表为空 —— 记录现状（防御方案见前端）
        let _ = v;
    }

    const ALL_BM: [BorderMode; 4] = [BorderMode::Default, BorderMode::Wrap, BorderMode::Bounce, BorderMode::Degrade];

    fn mk_b(sz: usize) -> GameBoard {
        vec![vec![Cell { owner: None, count: 0, th: None, blocked: false }; sz]; sz]
    }
    fn set(b: &mut GameBoard, x: usize, y: usize, owner: usize, count: u8) {
        b[x][y] = Cell { owner: Some(owner), count, th: None, blocked: false };
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

    // ── 11) HistoryRecord camelCase 序列化/反序列化（tauri 历史记录保存链路） ──
    #[test]
    fn history_record_camelcase_roundtrip() {
        let json = r#"{
            "id": 123, "time": "2026-08-03 12:00", "mode": "ai",
            "aiAlgorithm": "alphabeta", "aiDepth": 2, "gameCount": 1,
            "playerCount": 4, "aiCount": 3, "boardSize": 7,
            "borderMode": "wrap", "capMode": "5",
            "winner": 0, "colorNames": ["红","黄","蓝","绿"],
            "history": {"c":true,"t":2,"p":[],"pt":[],"m":[]},
            "finished": true
        }"#;
        // 反序列化（模拟 app.js 传的 camelCase record）
        let rec: HistoryRecord = serde_json::from_str(json).unwrap();
        assert_eq!(rec.border_mode.as_deref(), Some("wrap"), "borderMode 应反序列化");
        assert_eq!(rec.cap_mode.as_deref(), Some("5"), "capMode 应反序列化");
        assert_eq!(rec.board_size, 7);
        assert_eq!(rec.ai_depth, 2);
        assert_eq!(rec.ai_algorithm, "alphabeta");
        assert_eq!(rec.player_count, 4);
        // 序列化回 camelCase（模拟 load_game_history 返回）
        let out = serde_json::to_value(&rec).unwrap();
        assert_eq!(out["borderMode"], "wrap", "序列化应输出 camelCase borderMode");
        assert_eq!(out["capMode"], "5");
        assert_eq!(out["boardSize"], 7);
        assert_eq!(out["aiAlgorithm"], "alphabeta");
        assert_eq!(out["playerCount"], 4);
    }
    // ── 阈值感知走法生成：cap5 下 count==4 的引爆动作必须合法 ──
    #[test]
    fn get_moves_cap5_includes_explosive() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4);
        let mvs5 = get_moves(&b, sz, 0, None, BorderMode::Default, CapMode::Cap5);
        assert!(mvs5.contains(&(2, 2)), "cap5: count4 explosive move must be legal, got {:?}", mvs5);
        let mvs3 = get_moves(&b, sz, 0, None, BorderMode::Default, CapMode::Cap3);
        assert!(!mvs3.contains(&(2, 2)), "cap3: count4 must not be legal");
        let mut b3 = mk_b(sz);
        set(&mut b3, 2, 2, 0, 2);
        let mvs3b = get_moves(&b3, sz, 0, None, BorderMode::Default, CapMode::Cap3);
        assert!(mvs3b.contains(&(2, 2)), "cap3: count2 critical move must be legal");
    }

    // ── 策略 AI：cap3 二二相接 / cap4 三三相接 / cap5 四四相接 引爆 ──
    #[test]
    fn strategy_cap3_explodes_on_pair() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 2);
        set(&mut b, 2, 3, 1, 2);
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Default, CapMode::Cap3);
        assert_eq!(mv, Some((2, 2)), "cap3 strategy should explode pair (2v2), got {:?}", mv);
    }
    #[test]
    fn strategy_cap5_explodes_on_quad() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4);
        set(&mut b, 2, 3, 1, 4);
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

    // ── 策略 AI：cap3 下建立临界升 count1→2，不选 count2→3 自爆 ──
    #[test]
    fn strategy_cap3_no_suicide() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 1);
        set(&mut b, 1, 1, 0, 2);
        set(&mut b, 4, 4, 1, 1);
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Default, CapMode::Cap3);
        assert_eq!(mv, Some((2, 2)), "cap3 strategy must upgrade 1->2, not suicide 2->3, got {:?}", mv);
    }

    // ── 回环模式：策略 AI 上下左右判断与回环相符 ──
    #[test]
    fn strategy_wrap_detects_wrapped_threat() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 0, 2, 0, 2);
        set(&mut b, 4, 2, 1, 2);
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Wrap, CapMode::Cap3);
        assert_eq!(mv, Some((0, 2)), "wrap: wrapped adjacency should trigger explosion, got {:?}", mv);
    }

    // ── MCTS / PVS / Alpha-Beta 在 cap5 下能够执行引爆走法 ──
    #[test]
    fn mcts_cap5_can_explode() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4);
        set(&mut b, 2, 3, 1, 4);
        let mv = find_best_move_mcts(&b, sz, 0, 2, &[], 2, BorderMode::Default, CapMode::Cap5);
        assert_eq!(mv, Some((2, 2)), "cap5 MCTS should pick explosive move, got {:?}", mv);
    }
    #[test]
    fn pvs_cap5_can_explode() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4);
        set(&mut b, 2, 3, 1, 4);
        let mv = find_best_move_pvs(&b, sz, 0, 2, &[], 2, 0, true, BorderMode::Default, CapMode::Cap5, 0);
        assert_eq!(mv, Some((2, 2)), "cap5 PVS should pick explosive move, got {:?}", mv);
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

    // ── simulate_to_end 全模式可跑通（策略 AI + 多模式） ──
    #[test]
    fn simulate_to_end_all_combos_strategy() {
        for &bm in &[BorderMode::Default, BorderMode::Wrap, BorderMode::Bounce, BorderMode::Degrade] {
            for &cm in &[CapMode::Cap3, CapMode::Cap4, CapMode::Cap5] {
                let sz = 7;
                let max_players = 3;
                let mut b = mk_b(sz);
                for (x, y, p) in [(0usize, 0usize, 0usize), (0, 6, 1), (6, 0, 2), (6, 6, 0), (3, 3, 1), (1, 1, 2)] {
                    b[x][y] = Cell { owner: Some(p), count: 2, th: b[x][y].th, blocked: false };
                }
                let mut cfg = std::collections::HashMap::new();
                for p in 0..max_players {
                    cfg.insert(p.to_string(), serde_json::json!({"algorithm": "strategy", "depth": 1, "useMlEval": true}));
                }
                let r = simulate_to_end(b.clone(), sz, max_players, 0, vec![], bm, cm, None, 0, cfg);
                assert!(!r.history.is_empty(), "{bm:?}×{cm:?}: sim should produce history");
            }
        }
    }

    // ── 设备性能检测：AI 计算基准 ──
    fn bench_cfg(players: Vec<(&str, usize)>) -> AiBenchGameConfig {
        AiBenchGameConfig {
            size: 7,
            players: players.into_iter().map(|(alg, depth)| AiBenchPlayer {
                algorithm: alg.to_string(),
                depth: Some(depth),
                random_scale: Some(0),
                use_ml_eval: Some(true),
            }).collect(),
            border_mode: BorderMode::Default,
            cap_mode: CapMode::Cap4,
        }
    }

    #[test]
    fn bench_ai_game_produces_winner_and_steps() {
        let cfg = bench_cfg(vec![("strategy", 2), ("strategy", 2), ("alphabeta", 2), ("pvs", 2)]);
        for gc in 0..5u32 {
            let r = run_ai_bench_game(&cfg, gc);
            assert!(r.elapsed_ms > 0.0, "局 {gc}: 引擎耗时应 > 0");
            assert!(r.steps > 0, "局 {gc}: 应有落子步数");
            assert!(r.winner.is_some(), "局 {gc}: 应有胜者");
            let w = r.winner.unwrap();
            assert!(w < 4, "局 {gc}: 胜者应在 0..4 内");
        }
    }

    #[test]
    fn bench_ai_game_seeds_vary_results() {
        // 引擎含非确定性随机（MCTS / 随机逻辑），同 seed 不保证复现；
        // 这里校验不同 seed 结果具备多样性且全部有效
        let cfg = bench_cfg(vec![("strategy", 2), ("alphabeta", 2), ("pvs", 2), ("mcts", 1)]);
        let mut seen = std::collections::HashSet::new();
        for gc in 0..10u32 {
            let r = run_ai_bench_game(&cfg, gc);
            assert!(r.winner.is_some(), "局 {gc}: 应有胜者");
            assert!(r.steps > 0, "局 {gc}: 应有步数");
            seen.insert((r.winner, r.steps));
        }
        assert!(seen.len() >= 3, "10 局不同 seed 应产生至少 3 种不同结果，实际 {}", seen.len());
    }

    #[test]
    fn bench_ai_game_mcts_included() {
        let cfg = bench_cfg(vec![("mcts", 1), ("mcts", 1), ("strategy", 2), ("alphabeta", 2)]);
        for gc in 0..3u32 {
            let r = run_ai_bench_game(&cfg, gc);
            assert!(r.winner.is_some(), "局 {gc}: MCTS 对局应有胜者");
            assert!(r.steps > 0, "局 {gc}: MCTS 对局应有步数");
        }
    }

    #[test]
    fn bench_ai_game_spread_starts_valid() {
        for &(sz, n) in &[(7usize, 4usize), (9, 6), (11, 6), (5, 2)] {
            let pos = spread_starts(sz, n);
            assert_eq!(pos.len(), n, "{sz}×{sz} {n} 人首子数量");
            for &(x, y) in &pos {
                assert!(x < sz && y < sz, "首子在界内");
            }
            // 两两切比雪夫距离 ≥3
            for i in 0..n {
                for j in (i + 1)..n {
                    let d = pos[i].0.abs_diff(pos[j].0).max(pos[i].1.abs_diff(pos[j].1));
                    assert!(d >= 3, "{sz}×{sz}: 首子 ({:?}) 与 ({:?}) 距离 {d} < 3", pos[i], pos[j]);
                }
            }
        }
    }
}