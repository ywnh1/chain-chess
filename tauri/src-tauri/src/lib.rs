// Copyright (c) 2026 ywnh1
// SPDX-License-Identifier: MIT

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
    pub history: Vec<TurnHistory>,
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
        let eliminated = process_click(&mut b, size, x, y, player, max_players);
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
            chain_count: 0, // will be calculated separately
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
    fs::write(&*path, json).map_err(|e| e.to_string())?;
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
    fs::write(&path, json).map_err(|e| e.to_string())?;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
            process_move,
            save_game_history,
            load_game_history,
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

fn process_click(board: &mut GameBoard, sz: usize, x: usize, y: usize, player: usize, _max_players: usize) -> Vec<usize> {
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
            return vec![];
        }
    }

    // chain reaction (FIFO = VecDeque)
    let mut chain: VecDeque<(usize, usize)> = VecDeque::new();
    chain.push_back((x, y));
    while let Some((cx, cy)) = chain.pop_front() {
        if board[cx][cy].count >= 4 {
            board[cx][cy].count = 0;
            board[cx][cy].owner = None;
            for (nx, ny) in nbrs(cx, cy, sz) {
                board[nx][ny].owner = Some(player);
                board[nx][ny].count += 1;
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

    before.difference(&after).copied().collect()
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
        let elim = process_click(&mut self.board, self.sz, x, y, self.player, self.max_players);
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
        // no-op：alpha_beta 重写使用克隆替代 set/unset
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

// ─── Public entry point ───

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
