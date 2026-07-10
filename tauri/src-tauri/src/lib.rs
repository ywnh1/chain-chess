use std::collections::VecDeque;
use std::time::{Duration, Instant};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

// ─── Board types ───

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Cell {
    pub owner: Option<usize>,
    pub count: u8,
}

pub type GameBoard = Vec<Vec<Cell>>;

// ─── Tauri commands ───

#[tauri::command]
async fn ai_move(
    board: GameBoard,
    size: usize,
    player: usize,
    depth: usize,
    eliminated: Vec<usize>,
    max_players: usize,
) -> Result<[usize; 2], String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        find_best_move(&board, size, player, depth, &eliminated, max_players)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Some((x, y)) => Ok([x, y]),
        None => Err("No valid move".into()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ai_move])
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
fn nbrs8(i: usize, j: usize, sz: usize) -> Vec<(usize, usize)> {
    let mut r = Vec::with_capacity(8);
    if i > 0 && j > 0 { r.push((i - 1, j - 1)); }
    if i > 0 { r.push((i - 1, j)); }
    if i > 0 && j + 1 < sz { r.push((i - 1, j + 1)); }
    if j > 0 { r.push((i, j - 1)); }
    if j + 1 < sz { r.push((i, j + 1)); }
    if i + 1 < sz && j > 0 { r.push((i + 1, j - 1)); }
    if i + 1 < sz { r.push((i + 1, j)); }
    if i + 1 < sz && j + 1 < sz { r.push((i + 1, j + 1)); }
    r
}

// ─── Board helpers ───

fn has_pieces(board: &GameBoard, player: usize) -> bool {
    board.iter().flatten().any(|c| c.owner == Some(player))
}

fn near_any(board: &GameBoard, sz: usize, x: usize, y: usize) -> bool {
    nbrs8(x, y, sz).iter().any(|&(ni, nj)| board[ni][nj].owner.is_some())
}

fn process_click(board: &mut GameBoard, sz: usize, x: usize, y: usize, player: usize) -> Vec<usize> {
    // collect owners before
    let before: std::collections::HashSet<usize> = board
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
    let after: std::collections::HashSet<usize> = board
        .iter()
        .flatten()
        .filter_map(|c| c.owner)
        .collect();

    before.difference(&after).copied().collect()
}

fn eval_board(board: &GameBoard, player: usize) -> i32 {
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
    (my_score - opp_score) * 2 + (my_territory - opp_territory)
}

// ─── Move generation & ordering ───

fn get_moves(board: &GameBoard, sz: usize, player: usize) -> Vec<(usize, usize)> {
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
                if c.owner.is_none() && !near_any(board, sz, i, j) {
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

// ─── Serial Alpha-Beta search ───

fn alpha_beta(
    board: &GameBoard,
    sz: usize,
    player: usize,
    depth: usize,
    max_depth: usize,
    mut alpha: i32,
    mut beta: i32,
    ai_player: usize,
    start: Instant,
    limit: Duration,
    alive: &[usize],
    elim_set: &[usize],
) -> (i32, Option<(usize, usize)>) {
    if depth >= max_depth || start.elapsed() > limit {
        return (eval_board(board, ai_player), None);
    }

    let moves = get_moves(board, sz, player);
    if moves.is_empty() {
        return (eval_board(board, ai_player), None);
    }
    if moves.len() == 1 {
        return (eval_board(board, ai_player), Some(moves[0]));
    }

    let ordered = order_moves(moves, board, sz, player);
    let max_eval = ordered.len().min(10);

    let is_max = player == ai_player;
    let mut best_score = if is_max { i32::MIN } else { i32::MAX };
    let mut best_move: Option<(usize, usize)> = None;

    for k in 0..max_eval {
        let (i, j) = ordered[k];
        let mut nb = board.clone();
        let elim = process_click(&mut nb, sz, i, j, player);

        // merge eliminated
        let mut new_elim: Vec<usize> = elim_set.to_vec();
        for &e in &elim {
            if !new_elim.contains(&e) {
                new_elim.push(e);
            }
        }

        let alive_next: Vec<usize> = alive
            .iter()
            .filter(|p| !new_elim.contains(p))
            .copied()
            .collect();

        if alive_next.len() <= 1 {
            let sc = if is_max { i32::MAX - 1 } else { i32::MIN + 1 };
            if (is_max && sc > best_score) || (!is_max && sc < best_score) {
                best_score = sc;
                best_move = Some((i, j));
            }
            continue;
        }

        let idx = alive_next.iter().position(|&p| p == player).unwrap_or(0);
        let next_player = alive_next[(idx + 1) % alive_next.len()];

        let (score, _) = alpha_beta(
            &nb, sz, next_player, depth + 1, max_depth,
            alpha, beta, ai_player, start, limit,
            &alive_next, &new_elim,
        );

        if is_max {
            if score > best_score {
                best_score = score;
                best_move = Some((i, j));
            }
            alpha = alpha.max(score);
        } else {
            if score < best_score {
                best_score = score;
                best_move = Some((i, j));
            }
            beta = beta.min(score);
        }
        if beta <= alpha {
            break;
        }
    }
    (best_score, best_move)
}

// ─── Public entry point (Rayon parallel root) ───

pub fn find_best_move(
    board: &GameBoard,
    sz: usize,
    player: usize,
    depth: usize,
    eliminated: &[usize],
    max_players: usize,
) -> Option<(usize, usize)> {
    let start = Instant::now();
    let limit = Duration::from_millis(3000 + depth as u64 * 500);

    // single move → no search
    let all_moves = get_moves(board, sz, player);
    if all_moves.len() <= 1 {
        return all_moves.into_iter().next();
    }

    // first move (no pieces yet) → center preference, avoid edges
    if !has_pieces(board, player) {
        let mut candidates: Vec<(usize, usize)> = (1..sz - 1)
            .flat_map(|i| {
                (1..sz - 1).filter_map(move |j| {
                    if board[i][j].owner.is_none() && !near_any(board, sz, i, j) {
                        Some((i, j))
                    } else {
                        None
                    }
                })
            })
            .collect();
        if candidates.is_empty() {
            // fallback to all cells
            for i in 0..sz {
                for j in 0..sz {
                    if board[i][j].owner.is_none() && !near_any(board, sz, i, j) {
                        candidates.push((i, j));
                    }
                }
            }
        }
        if candidates.is_empty() {
            return None;
        }
        let cx = sz as f64 / 2.0 - 0.5;
        return candidates
            .into_iter()
            .min_by(|&(i1, j1), &(i2, j2)| {
                let d1 = (i1 as f64 - cx).abs() + (j1 as f64 - cx).abs();
                let d2 = (i2 as f64 - cx).abs() + (j2 as f64 - cx).abs();
                d1.partial_cmp(&d2).unwrap()
            });
    }

    // build alive list
    let alive: Vec<usize> = (0..max_players)
        .filter(|p| !eliminated.contains(p))
        .filter(|&p| p == player || has_pieces(board, p))
        .collect();
    if alive.len() <= 1 {
        return None;
    }

    // order moves for root
    let ordered = order_moves(get_moves(board, sz, player), board, sz, player);
    let max_eval = ordered.len().min(10);
    if max_eval == 0 {
        return None;
    }

    // ── Rayon parallel root search ──
    let results: Vec<(i32, usize, usize)> = (0..max_eval)
        .into_par_iter()
        .map(|k| {
            let (i, j) = ordered[k];
            let mut nb = board.clone();
            let elim = process_click(&mut nb, sz, i, j, player);

            let mut new_elim: Vec<usize> = eliminated.to_vec();
            for &e in &elim {
                if !new_elim.contains(&e) {
                    new_elim.push(e);
                }
            }

            let alive_next: Vec<usize> = alive
                .iter()
                .filter(|p| !new_elim.contains(p))
                .copied()
                .collect();

            if alive_next.len() <= 1 {
                return (i32::MAX - 1, i, j);
            }

            let idx = alive_next.iter().position(|&p| p == player).unwrap_or(0);
            let next_player = alive_next[(idx + 1) % alive_next.len()];

            let (score, _) = alpha_beta(
                &nb, sz, next_player, 1, depth,
                i32::MIN, i32::MAX, player, start, limit,
                &alive_next, &new_elim,
            );
            (score, i, j)
        })
        .collect();

    // pick best
    results
        .into_iter()
        .max_by_key(|&(score, _, _)| score)
        .map(|(_, i, j)| (i, j))
}
