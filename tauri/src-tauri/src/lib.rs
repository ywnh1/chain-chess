// Copyright (c) 2026 ywnh1
// SPDX-License-Identifier: MIT

use std::io::Write;
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use alpha_beta_pruning::{AlphaBeta, Grade};

// ─── Board types ───

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Cell {
    pub owner: Option<usize>,
    pub count: u8,
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
}

// ─── Process move result ───

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMoveResult {
    pub board: GameBoard,
    pub eliminated: Vec<usize>,
    pub chain_count: u32,
    pub game_over: bool,
    pub winner: Option<usize>,
}

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
) -> Result<ProcessMoveResult, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut b = board.clone();
        let (eliminated, chain_count) = process_click(&mut b, size, x, y, player, max_players);
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
            game_over,
            winner,
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
    game_count: u32,
    first_move_pos: Option<[usize; 2]>,
) -> Result<[usize; 2], String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        find_best_move(&board, size, player, depth, &eliminated, max_players, game_count, first_move_pos)
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
    board: GameBoard,
    size: usize,
    player: usize,
    depth: usize,
    eliminated: Vec<usize>,
    max_players: usize,
    game_count: u32,
    first_move_pos: Option<[usize; 2]>,
    algorithm: String,
) -> Result<[usize; 2], String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        if algorithm == "pvs" {
            // PVS (NegaMax) + Killer/History + QSearch
            let mut searcher = PvsSearcher::new();
            searcher.find_best(&board, size, player, max_players, &eliminated, depth)
        } else {
            // 默认使用 Alpha-Beta（原 find_best_move）
            find_best_move(&board, size, player, depth, &eliminated, max_players, game_count, first_move_pos)
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
async fn ai_move_mcts(
    board: GameBoard,
    size: usize,
    player: usize,
    depth: usize,
    eliminated: Vec<usize>,
    max_players: usize,
) -> Result<[usize; 2], String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        find_best_move_mcts(&board, size, player, depth, &eliminated, max_players)
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
    let history: Vec<HistoryRecord> = serde_json::from_str(&content).unwrap_or_default();
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
async fn clear_game_history(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = state.history_file.lock().map_err(|e| e.to_string())?;
    if path.exists() {
        fs::remove_file(&*path).map_err(|e| e.to_string())?;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
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
            process_move,
            save_game_history,
            load_game_history,
            import_game_history,
            export_game_history_dialog,
            clear_game_history,
            exit_app,
            save_round_history,
            load_round_history,
            clear_round_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ─── Neighbors ───

#[inline]
fn nbrs(i: usize, j: usize, sz: usize) -> Vec<(usize, usize)> {
    let mut r = Vec::with_capacity(4);
    if i > 0 { r.push((i - 1, j)); }
    if i + 1 < sz { r.push((i + 1, j)); }
    if j > 0 { r.push((i, j - 1)); }
    if j + 1 < sz { r.push((i, j + 1)); }
    r
}


#[inline]
fn has_pieces(board: &GameBoard, player: usize) -> bool {
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

fn process_click(board: &mut GameBoard, sz: usize, x: usize, y: usize, player: usize, _max_players: usize) -> (Vec<usize>, u32) {
    // collect owners before
    let before: HashSet<usize> = board
        .iter()
        .flatten()
        .filter_map(|c| c.owner)
        .collect();

    // check first move (no pieces yet) BEFORE mutable borrow
    let is_first = board[x][y].owner.is_none() && !has_pieces(board, player);

    // place / upgrade
    {
        let cell = &mut board[x][y];
        if cell.owner.is_none() {
            cell.owner = Some(player);
            cell.count = if is_first { 3 } else { 1 };
        } else if cell.owner == Some(player) {
            cell.count += 1;
        } else {
            return (vec![], 0);
        }
    }

    // chain reaction (FIFO = VecDeque)
    let mut chain: VecDeque<(usize, usize)> = VecDeque::new();
    chain.push_back((x, y));
    let mut chain_count: u32 = 0;
    while let Some((cx, cy)) = chain.pop_front() {
        if board[cx][cy].count >= 4 {
            board[cx][cy].count = 0;
            board[cx][cy].owner = None;
            chain_count += 1;
            for (nx, ny) in nbrs(cx, cy, sz) {
                board[nx][ny].owner = Some(player);
                board[nx][ny].count = board[nx][ny].count.saturating_add(1);
                chain.push_back((nx, ny));
            }
        }
    }

    // owners after
    let after: HashSet<usize> = board
        .iter()
        .flatten()
        .filter_map(|c| c.owner)
        .collect();

    (before.difference(&after).copied().collect(), chain_count)
}

fn eval_board(board: &GameBoard, player: usize, game_count: u32) -> i32 {
    let mut my_score = 0i32;
    let mut opp_score = 0i32;
    let mut my_territory = 0i32;
    let mut opp_territory = 0i32;

    for row in board {
        for cell in row {
            match cell.owner {
                Some(p) if p == player => {
                    my_score += cell.count as i32;
                    my_territory += 1;
                }
                Some(_) => {
                    opp_score += cell.count as i32;
                    opp_territory += 1;
                }
                None => {}
            }
        }
    }

    let base = (my_score - opp_score) * 2 + (my_territory - opp_territory);

    // 早期游戏（<7局）加入随机值，增加探索性
    // 游戏开始时随机值较大，逐渐减小，5-7局后归零
    let random_scale = if game_count < 5 {
        (5 - game_count) as f64 * 8.0
    } else if game_count < 7 {
        (7 - game_count) as f64 * 2.0
    } else {
        0.0
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

fn get_moves(board: &GameBoard, sz: usize, player: usize, _first_move_pos: Option<(usize, usize)>) -> Vec<(usize, usize)> {
    let has_p = has_pieces(board, player);
    let mut moves = Vec::new();
    for i in 0..sz {
        for j in 0..sz {
            let c = &board[i][j];
            if has_p {
                if c.owner == Some(player) && c.count < 4 {
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

fn order_moves(moves: Vec<(usize, usize)>, board: &GameBoard, sz: usize, player: usize) -> Vec<(usize, usize)> {
    let mut scored: Vec<(i32, (usize, usize))> = moves
        .into_iter()
        .map(|(i, j)| {
            let c = &board[i][j];
            let mut score = c.count as i32 * 10;
            if c.count >= 3 {
                score += 100;
            }
            let near_opp: i32 = nbrs(i, j, sz)
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
}

impl AlphaBeta<(usize, usize)> for GameState {
    fn evaluate(&self) -> Grade {
        Grade::Score(eval_board(&self.board, self.ai_player, self.game_count) as i64)
    }

    fn get_moves(&self) -> Vec<(usize, usize)> {
        get_moves(&self.board, self.sz, self.player, None)
    }

    fn set(&mut self, m: &(usize, usize)) {
        let (x, y) = *m;
        let (elim, _chain) = process_click(&mut self.board, self.sz, x, y, self.player, self.max_players);
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

        // 走法排序 + 限制前10
        let ordered = order_moves(all_moves, &self.board, self.sz, self.player);
        let max_eval = ordered.len().min(10);
        if max_eval == 0 { return None; }

        // Rayon 并行根搜索（利用 crate 的 Grade + AlphaBeta trait）
        ordered[..max_eval].par_iter()
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

    /// 重写 alpha_beta：使用克隆替代 set/unset，支持多玩家轮换
    fn alpha_beta(&mut self, mut alpha: Grade, mut beta: Grade, depth: usize, _is_max: bool) -> Grade {
        let moves = self.get_moves();
        if depth == 0 || moves.is_empty() {
            return self.evaluate();
        }
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
const PVS_HISTORY_SIZE: usize = 20;

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
    killers: [[Option<(usize, usize)>; PVS_MAX_DEPTH]; 2],
    history: [[i32; PVS_HISTORY_SIZE]; PVS_HISTORY_SIZE],
}

impl PvsSearcher {
    fn new() -> Self {
        Self {
            killers: [[None; PVS_MAX_DEPTH]; 2],
            history: [[0; PVS_HISTORY_SIZE]; PVS_HISTORY_SIZE],
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
                if c.owner == Some(player) && c.count < 4 {
                    let mut score = c.count as i64 * 10;
                    if c.count >= 3 { score += 100; }
                    // 邻居对手分数
                    for &(ni, nj) in &nbrs(i, j, sz) {
                        let nc = &board[ni][nj];
                        if nc.owner.is_some() && nc.owner != Some(player) {
                            score += nc.count as i64 * 5;
                        }
                    }
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
                    // 邻居对手分数
                    for &(ni, nj) in &nbrs(i, j, sz) {
                        let nc = &board[ni][nj];
                        if nc.owner.is_some() && nc.owner != Some(player) {
                            score += nc.count as i64 * 5;
                        }
                    }
                    moves.push((score, (i, j)));
                }
            }}
        }
        moves.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        moves.into_iter().map(|(_, m)| m).collect()
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
        if depth >= 20 { return eval_board(board, player, 0); }
        let stand_pat = eval_board(board, player, 0);
        if stand_pat >= beta { return beta; }
        let mut alpha = if stand_pat > alpha { stand_pat } else { alpha };

        // 收集 count >= 3 的己方棋子（爆炸性走法）
        // 用局部数组避免 Vec 分配
        let mut moves_buf = [(0usize, 0usize); 100];
        let mut n = 0usize;
        for i in 0..sz { for j in 0..sz {
            let c = &board[i][j];
            if c.owner == Some(player) && c.count >= 3 && c.count < 4 {
                if n < moves_buf.len() { moves_buf[n] = (i, j); n += 1; }
            }
        }}

        for &m in &moves_buf[..n] {
            let mut child = board.clone();
            let (new_elim, _) = process_click(&mut child, sz, m.0, m.1, player, max_players);
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
    ) -> i32 {
        if depth == 0 {
            return self.quiescence(board, sz, player, max_players, elim, alpha, beta, 0);
        }

        let moves = self.get_moves_ordered(board, sz, player, depth);
        let max_branch = moves.len().min(10);
        if max_branch == 0 { return eval_board(board, player, 0); }

        let mut best_score = i32::MIN + 1;

        for (idx, &m) in moves[..max_branch].iter().enumerate() {
            let mut child = board.clone();
            let (new_elim, _) = process_click(&mut child, sz, m.0, m.1, player, max_players);
            let mut child_elim = elim;
            for &e in &new_elim { child_elim.add(e); }
            let next = next_live_player_es(&child, sz, player, child_elim, max_players);

            let score = if idx == 0 {
                -self.pvs(&child, sz, next, max_players, child_elim, depth - 1, -beta, -alpha)
            } else {
                let s = -self.pvs(&child, sz, next, max_players, child_elim, depth - 1, -alpha - 1, -alpha);
                if s > alpha && s < beta {
                    -self.pvs(&child, sz, next, max_players, child_elim, depth - 1, -beta, -alpha)
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

        let max_branch = ordered.len().min(10);
        let elim_root = ElimSet::from_slice(eliminated);

        // ─── Warmup: 浅层搜索填充 killer/history 表 ───
        let warmup_n = max_branch.min(3);
        let warm_depth = if depth >= 3 { 2 } else { depth.saturating_sub(1) };
        if warm_depth > 0 {
            for &m in ordered[..warmup_n].iter() {
                let mut child = board.clone();
                let (new_elim, _) = process_click(&mut child, sz, m.0, m.1, player, max_players);
                let mut child_elim = elim_root;
                for &e in &new_elim { child_elim.add(e); }
                let next = next_live_player_es(&child, sz, player, child_elim, max_players);
                self.pvs(&child, sz, next, max_players, child_elim, warm_depth, i32::MIN + 1, i32::MAX - 1);
            }
        }

        // 快照预热后的 killer/history（Copy 类型，闭包按值捕获）
        let base_killers = self.killers;
        let base_history = self.history;

        // ─── 根级并行搜索（每个分支自带预热数据） ───
        let results: Vec<_> = ordered[..max_branch]
            .par_iter()
            .map(|&m| {
                let mut child = board.clone();
                let (new_elim, _) = process_click(&mut child, sz, m.0, m.1, player, max_players);
                let mut child_elim = elim_root;
                for &e in &new_elim { child_elim.add(e); }
                let next = next_live_player_es(&child, sz, player, child_elim, max_players);

                let mut searcher = PvsSearcher {
                    killers: base_killers,
                    history: base_history,
                };
                let score = if depth > 0 {
                    -searcher.pvs(&child, sz, next, max_players, child_elim, depth - 1, i32::MIN + 1, i32::MAX - 1)
                } else {
                    eval_board(&child, player, 0)
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
const UCB_C: f64 = 1.414;
const MCTS_ITER_PER_DEPTH: usize = 1200; // depth=1 -> 1200, depth=10 -> 12000
const MCTS_PLAYOUT_MAX: usize = 60;
const MCTS_TREE_MAX_NODES: usize = 3000; // 每棵树最大节点数（防内存暴涨）

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
}

impl MctsTree {
    fn new(board: GameBoard, eliminated: Vec<usize>, player: usize, sz: usize, max_players: usize, ai_player: usize, max_nodes: usize) -> Self {
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
    fn untried_moves(&self, node: usize) -> Vec<(usize, usize)> {
        let all = get_moves(&self.boards[node], self.sz, self.players[node], None);
        let tried: std::collections::HashSet<(usize, usize)> =
            self.children[node].iter().map(|e| (e.x, e.y)).collect();
        all.into_iter().filter(|m| !tried.contains(m)).collect()
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

    /// 展开一个节点：从未试走法中选一个（依 order_moves 排序），创建子节点
    fn expand(&mut self, node: usize, _rng: &mut XorShift) -> Option<usize> {
        if self.visits.len() >= self.max_nodes { return None; }
        let mut untried = self.untried_moves(node);
        if untried.is_empty() { return None; }

        // 用 order_moves 排序后，优先展开最有希望的走法
        untried = order_moves(untried, &self.boards[node], self.sz, self.players[node]);
        let (mx, my) = untried[0];

        // 应用走法得到新状态
        let mut new_board = self.boards[node].clone();
        let mut new_elim = self.eliminateds[node].clone();
        let (new_elim_players, _) = process_click(&mut new_board, self.sz, mx, my, self.players[node], self.max_players);
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
            if !untried.is_empty() {
                // 有未试走法 → EXPAND
                if let Some(new_node) = self.expand(leaf, rng) {
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

        let moves = get_moves(&b, sz, cur, None);
        if moves.is_empty() {
            cur = next_live_player(&b, sz, cur, &elim, max_players);
            continue;
        }

        let idx = rng.next_usize(moves.len());
        let (x, y) = moves[idx];
        let (new_elim, _) = process_click(&mut b, sz, x, y, cur, max_players);
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
    first_move_pos: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    let all_moves = get_moves(board, sz, ai_player, first_move_pos);
    if all_moves.is_empty() { return None; }
    if all_moves.len() == 1 { return Some(all_moves[0]); }

    // 首步居中偏好
    if !has_pieces(board, ai_player) {
        return first_move_center(board, sz);
    }

    // 走法排序，分枝限制
    let ordered = order_moves(all_moves, board, sz, ai_player);
    let branches = ordered.len().min(15);
    let iters_per = (iterations / branches).max(50);
    let max_nodes = MCTS_TREE_MAX_NODES / branches.max(1);

    // ─── Root-parallel MCTS ───
    // 每个根级候选走法获得独立的搜索树，在子树内执行完整 MCTS
    let results: Vec<_> = ordered[..branches].par_iter().map(|&(mx, my)| {
        // 应用根走法得到子树根状态
        let mut b = board.clone();
        let mut elim = eliminated.to_vec();
        let (new_elim, _) = process_click(&mut b, sz, mx, my, ai_player, max_players);
        for &e in &new_elim {
            if !elim.contains(&e) { elim.push(e); }
        }
        let next_p = next_live_player(&b, sz, ai_player, &elim, max_players);

        // 构建子树
        let mut tree = MctsTree::new(b, elim, next_p, sz, max_players, ai_player, max_nodes);
        let mut rng = XorShift::seed(
            mx as u64 * 6364136223846793005 + my as u64 * 1442695040888963407 + 12345,
        );

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
) -> Option<(usize, usize)> {
    let iterations = depth.max(1) * MCTS_ITER_PER_DEPTH;
    mcts_search(board, sz, player, iterations, eliminated, max_players, None)
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
) -> Option<(usize, usize)> {
    let state = GameState {
        board: board.clone(),
        sz,
        player,
        ai_player: player,
        eliminated: eliminated.to_vec(),
        max_players,
        game_count,
    };
    state.run(depth)
}
