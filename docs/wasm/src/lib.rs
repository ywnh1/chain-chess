// Copyright (c) 2026 ywnh1
// SPDX-License-Identifier: MIT
//
// 连锁棋 PWA 游戏引擎（WASM）——宿主层。
// 引擎逻辑已经抽到 ../engine（chain_chess_core），这个 crate 只留：
//  - WASM 导出层（JSON 进出，供 pwa/engine.js 调用）
//  - PWA 侧专属的测试
// Tauri 专用代码（存储、更新、对话框）不在此 crate，由 pwa/engine.js 负责。
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use serde::{Deserialize, Serialize};
use chain_chess_core::*;


// ═══════ WASM 导出（JSON 进出） ═══════
#[cfg(target_arch = "wasm32")]
mod wasm_exports {
    use super::*;
    use wasm_bindgen::prelude::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct MoveReq {
        board: GameBoard,
        size: usize,
        x: usize,
        y: usize,
        player: usize,
        max_players: usize,
        border_mode: BorderMode,
        #[serde(default)]
        cap_mode: Option<CapMode>,
        #[serde(default)]
        seed: Option<u64>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AiReq {
        board: GameBoard,
        size: usize,
        player: usize,
        depth: usize,
        eliminated: Vec<usize>,
        max_players: usize,
        border_mode: BorderMode,
        #[serde(default)]
        cap_mode: Option<CapMode>,
        game_count: u32,
        first_move_pos: Option<[usize; 2]>,
        use_ml_eval: Option<bool>,
        algorithm: Option<String>,
        random_scale: Option<u32>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SimReq {
        board: GameBoard,
        size: usize,
        max_players: usize,
        cur_player: usize,
        eliminated: Vec<usize>,
        border_mode: BorderMode,
        #[serde(default)]
        cap_mode: Option<CapMode>,
        first_move_pos: Option<[usize; 2]>,
        game_count: u32,
        ai_configs: std::collections::HashMap<String, serde_json::Value>,
    }

    fn ok<T: Serialize>(v: &T) -> String {
        serde_json::json!({"ok": true, "data": v}).to_string()
    }
    fn err(msg: &str) -> String {
        serde_json::json!({"ok": false, "error": msg}).to_string()
    }

    /// process_move 命令
    #[wasm_bindgen]
    pub fn process_move_cmd(json: &str) -> String {
        let r: MoveReq = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return err(&format!("参数解析失败: {}", e)),
        };
        let mut b = r.board.clone();
        let cap_mode = r.cap_mode.unwrap_or_default();
        let (eliminated, chain_count, killed_by, steps, exploded) =
            process_click_with_snapshots(&mut b, r.size, r.x, r.y, r.player, r.max_players, r.border_mode, cap_mode, r.seed);
        let game_over = eliminated.len() >= r.max_players.saturating_sub(1);
        let winner = if game_over {
            let alive: Vec<usize> = (0..r.max_players)
                .filter(|p| !eliminated.contains(p) && has_pieces(&b, *p))
                .collect();
            alive.first().copied()
        } else {
            None
        };
        ok(&ProcessMoveResult { board: b, eliminated, killed_by, chain_count, game_over, winner, steps, exploded })
    }

    /// AI 走棋命令（按 algorithm 分派：mcts / pvs / alphabeta / strategy）
    #[wasm_bindgen]
    pub fn ai_move_cmd(json: &str) -> String {
        let r: AiReq = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return err(&format!("参数解析失败: {}", e)),
        };
        let use_ml = r.use_ml_eval.unwrap_or(true);
        let alg = r.algorithm.as_deref().unwrap_or("alphabeta");
        let rnd = r.random_scale.unwrap_or(0);
        let cap_mode = r.cap_mode.unwrap_or_default();
        let mv = match alg {
            "mcts" => find_best_move_mcts(&r.board, r.size, r.player, r.depth, &r.eliminated, r.max_players, r.border_mode, cap_mode),
            "pvs" => find_best_move_pvs(&r.board, r.size, r.player, r.depth, &r.eliminated, r.max_players, r.game_count, use_ml, r.border_mode, cap_mode, rnd),
            "strategy" => find_best_move_strategy(&r.board, r.size, r.player, &r.eliminated, r.max_players, r.game_count, r.first_move_pos, r.border_mode, cap_mode),
            _ => find_best_move(&r.board, r.size, r.player, r.depth, &r.eliminated, r.max_players, r.game_count, r.first_move_pos, use_ml, r.border_mode, cap_mode, rnd),
        };
        match mv {
            Some((x, y)) => ok(&[x, y]),
            None => err("No valid move"),
        }
    }

    /// 一键终局命令
    #[wasm_bindgen]
    pub fn simulate_to_end_cmd(json: &str) -> String {
        let r: SimReq = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return err(&format!("参数解析失败: {}", e)),
        };
        let result = simulate_to_end(
            r.board, r.size, r.max_players, r.cur_player, r.eliminated,
            r.border_mode, r.cap_mode.unwrap_or_default(), r.first_move_pos, r.game_count, r.ai_configs,
        );
        ok(&result)
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BenchPlayer {
        algorithm: String,
        #[serde(default)]
        depth: Option<usize>,
        #[serde(default)]
        random_scale: Option<u32>,
        #[serde(default)]
        use_ml_eval: Option<bool>,
    }

    fn default_bench_border() -> BorderMode { BorderMode::Default }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BenchReq {
        size: usize,
        players: Vec<BenchPlayer>,
        #[serde(default)]
        game_count: u32,
        #[serde(default = "default_bench_border")]
        border_mode: BorderMode,
        #[serde(default)]
        cap_mode: CapMode,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct BenchResult {
        elapsed_ms: f64,
        steps: usize,
        winner: Option<usize>,
    }

    /// 单局 AI 基准命令（与 Tauri 端 bench_ai_game 语义一致，供设备性能检测页使用）
    #[wasm_bindgen]
    pub fn bench_ai_game_cmd(json: &str) -> String {
        let r: BenchReq = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return err(&format!("参数解析失败: {}", e)),
        };
        let sz = r.size;
        let max_players = r.players.len();
        if sz < 5 || max_players < 2 {
            return ok(&BenchResult { elapsed_ms: 0.0, steps: 0, winner: None });
        }
        let mut board: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None, blocked: false }; sz]; sz];
        let starts = spread_starts(sz, max_players);
        // 首子等级 = 阈值 n-1（cap3→2、cap4→3、cap5→4；随机模式取中间等级 3）
        let th: u32 = match r.cap_mode {
            CapMode::Cap3 => 3,
            CapMode::Cap4 => 4,
            CapMode::Cap5 => 5,
            CapMode::Random => 3,
        };
        for (p, &(x, y)) in starts.iter().enumerate() {
            board[x][y] = Cell { owner: Some(p), count: (th - 1) as u8, th: None, blocked: false };
        }
        let mut ai_configs: HashMap<String, serde_json::Value> = HashMap::new();
        for (p, pl) in r.players.iter().enumerate() {
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
        // 计时由 engine.js 用 performance.now() 完成（wasm32-unknown-unknown 无标准时钟）
        let res = simulate_to_end(board, sz, max_players, 0, Vec::new(), r.border_mode, r.cap_mode, None, r.game_count, ai_configs);
        ok(&BenchResult { elapsed_ms: 0.0, steps: res.history.len(), winner: res.winner })
    }

    #[wasm_bindgen]
    pub fn engine_version() -> String {
        "chain-chess-engine-wasm-3.2.3".to_string()
    }
}

// ═══════════════════ 规则完备性测试（borderMode × capMode 全组合） ═══════════════════
#[cfg(test)]
mod tests {
    use super::*;

    const ALL_BM: [BorderMode; 5] = [BorderMode::Default, BorderMode::Wrap, BorderMode::Bounce, BorderMode::Degrade, BorderMode::Random];
    const ALL_CM: [CapMode; 4] = [CapMode::Cap3, CapMode::Cap4, CapMode::Cap5, CapMode::Random];

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

    // ── 7) simulate_to_end 全组合可跑通 + mv 重放一致 ──
    #[test]
    fn simulate_to_end_all_combos_replayable() {
        for &bm in &ALL_BM {
            for &cm in &ALL_CM {
                let sz = 7;
                let max_players = 3;
                let mut b = mk_b(sz);
                for (x, y, p) in [(0usize, 0usize, 0usize), (0, 6, 1), (6, 0, 2), (6, 6, 0), (3, 3, 1), (1, 1, 2)] {
                    b[x][y] = Cell { owner: Some(p), count: 3, th: b[x][y].th, blocked: false };
                }
                let mut cfg = std::collections::HashMap::new();
                for p in 0..max_players {
                    cfg.insert(p.to_string(), serde_json::json!({"algorithm": "strategy", "depth": 1, "useMlEval": true}));
                }
                let r = simulate_to_end(b.clone(), sz, max_players, 0, vec![], bm, cm, None, 0, cfg);
                assert!(!r.history.is_empty(), "{bm:?}×{cm:?}: sim should produce history");
                assert!(r.winner.is_some() || !r.eliminated_order.is_empty(), "{bm:?}×{cm:?}: sim should conclude");
                // mv 重放一致性
                let mut rb = b.clone();
                for h in &r.history {
                    let mv = h.mv.expect("mv with seed");
                    let (x, y, pl, seed) = (mv[0] as usize, mv[1] as usize, mv[2] as usize, mv[3]);
                    process_click_with_killer(&mut rb, sz, x, y, pl, max_players, bm, cm, Some(seed));
                }
                assert_eq!(rb, r.board, "{bm:?}×{cm:?}: replay must match simulation");
            }
        }
    }

    // ── 8) 首子等级 = 阈值 n-1（临界态） ──
    #[test]
    fn first_move_level_n_minus_1() {
        let sz = 5;
        // cap3：首子 2 级
        let b = mk_b(sz);
        let (nb, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap3, Some(1));
        assert_eq!(nb[2][2].count, 2, "cap3 首子应为 2 级");
        assert_eq!(nb[2][2].owner, Some(0));
        // cap4：首子 3 级
        let b4 = mk_b(sz);
        let (nb4, _, _) = do_click(&b4, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap4, Some(1));
        assert_eq!(nb4[2][2].count, 3, "cap4 首子应为 3 级");
        // cap5：首子 4 级
        let b5 = mk_b(sz);
        let (nb5, _, _) = do_click(&b5, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Cap5, Some(1));
        assert_eq!(nb5[2][2].count, 4, "cap5 首子应为 4 级");
        // 首子 n-1 不触发爆炸
        assert!(nb[2][2].owner.is_some(), "cap3 首子 2 级不应爆炸");
    }

    // ── 9) 随机阈值模式（capMode=random）：每步随机 3/4/5，seed 确定可复现 ──
    #[test]
    fn capmode_random_thresholds() {
        let sz = 5;
        // 3 子 +1：阈值 3 时炸；阈值 4/5 时不炸
        let mut th3 = false; let mut th5 = false;
        for seed in 0..60u64 {
            let mut b3 = mk_b(sz); set(&mut b3, 2, 2, 0, 2);
            let (n3, _, _) = do_click(&b3, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Random, Some(seed));
            if n3[2][2].owner.is_none() { th3 = true; } // 阈值=3：2+1=3 炸
            let mut b5 = mk_b(sz); set(&mut b5, 2, 2, 0, 3);
            let (n5, _, _) = do_click(&b5, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Random, Some(seed));
            if n5[2][2].owner.is_some() && n5[2][2].count == 4 { th5 = true; } // 阈值=5：3+1=4 不炸
        }
        assert!(th3, "随机阈值应出现 3");
        assert!(th5, "随机阈值应出现 5");
        // 同 seed 确定性
        let mut b = mk_b(sz); set(&mut b, 2, 2, 0, 2);
        let (r1, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Random, Some(7));
        let (r2, _, _) = do_click(&b, sz, 2, 2, 0, 2, BorderMode::Default, CapMode::Random, Some(7));
        assert_eq!(r1, r2, "同 seed 随机阈值应一致");
    }

    // ── 10) 随机边界模式（borderMode=random）：每步随机边界行为，seed 确定可复现 ──
    #[test]
    fn bordermode_random_runs() {
        let sz = 5;
        // 角落爆炸：不同边界扩散目标不同（default 2 方向 vs wrap 回环 4 方向等）
        let mut b = mk_b(sz);
        set(&mut b, 0, 0, 0, 3);
        let (nb, _, _) = do_click(&b, sz, 0, 0, 0, 2, BorderMode::Random, CapMode::Cap4, Some(5));
        assert!(nb[0][0].owner.is_none(), "随机边界下 cap4 3+1=4 应炸");
        // 同 seed 确定性
        let (r1, _, _) = do_click(&b, sz, 0, 0, 0, 2, BorderMode::Random, CapMode::Cap4, Some(5));
        let (r2, _, _) = do_click(&b, sz, 0, 0, 0, 2, BorderMode::Random, CapMode::Cap4, Some(5));
        assert_eq!(r1, r2, "同 seed 随机边界应一致");
        // 不同 seed 出现不同扩散（回环/反弹等不同目标集合）
        let mut diff = false;
        let base = do_click(&b, sz, 0, 0, 0, 2, BorderMode::Random, CapMode::Cap4, Some(1)).0;
        for seed in 0..20u64 {
            let r = do_click(&b, sz, 0, 0, 0, 2, BorderMode::Random, CapMode::Cap4, Some(seed)).0;
            if r != base { diff = true; break; }
        }
        assert!(diff, "随机边界不同 seed 应有不同扩散结果");
    }

    // ── 11) 阈值感知走法生成：cap5 下 count==4 的引爆动作必须合法 ──
    #[test]
    fn get_moves_cap5_includes_explosive() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4); // 己方临界 count4（再落一子即炸）
        let mvs5 = get_moves(&b, sz, 0, None, BorderMode::Default, CapMode::Cap5);
        assert!(mvs5.contains(&(2, 2)), "cap5: count4 explosive move must be legal, got {:?}", mvs5);
        // cap3 下 count4 不可能存在，也不应被允许
        let mvs3 = get_moves(&b, sz, 0, None, BorderMode::Default, CapMode::Cap3);
        assert!(!mvs3.contains(&(2, 2)), "cap3: count4 must not be legal");
        // cap3 下临界 count2 的引爆动作合法（count < 3）
        let mut b3 = mk_b(sz);
        set(&mut b3, 2, 2, 0, 2);
        let mvs3b = get_moves(&b3, sz, 0, None, BorderMode::Default, CapMode::Cap3);
        assert!(mvs3b.contains(&(2, 2)), "cap3: count2 critical move must be legal");
    }

    // ── 12) 策略 AI：cap3 模式“二二相接”引爆、cap5 模式“四四相接”引爆 ──
    #[test]
    fn strategy_cap3_explodes_on_pair() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 2); // 己方临界
        set(&mut b, 2, 3, 1, 2); // 对手临界相邻
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Default, CapMode::Cap3);
        assert_eq!(mv, Some((2, 2)), "cap3 strategy should explode pair (2v2), got {:?}", mv);
    }
    #[test]
    fn strategy_cap5_explodes_on_quad() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4); // 己方临界
        set(&mut b, 2, 3, 1, 4); // 对手临界相邻
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

    // ── 13) 策略 AI：cap3 下“建立临界”升 count1→2，不选 count2→3 的自爆走法 ──
    #[test]
    fn strategy_cap3_no_suicide() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 1); // 己方 count1（升到 2 安全）
        set(&mut b, 1, 1, 0, 2); // 己方 count2（cap3 下再落子自爆）
        set(&mut b, 4, 4, 1, 1); // 远处对手
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Default, CapMode::Cap3);
        assert_eq!(mv, Some((2, 2)), "cap3 strategy must upgrade 1->2, not suicide 2->3, got {:?}", mv);
    }

    // ── 14) 回环模式：策略 AI 的上下左右判断与回环相符（最上行上方=最下行） ──
    #[test]
    fn strategy_wrap_neighbors_loop() {
        let sz = 5;
        let mut b = mk_b(sz);
        // 己方 (0,2) count1（cap3 下安全升级位）；对手临界 count2 在 (4,2)（wrap 中位于其正上方）
        set(&mut b, 0, 2, 0, 1);
        set(&mut b, 4, 2, 1, 2);
        // wrap：对手临界贴着己方安全位 → 步骤2 排除（不安全），应走建立临界（也是 (0,2)）或引爆
        let mv_wrap = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Wrap, CapMode::Cap3);
        // 不允许升级到会引爆的位置？(0,2) 1->2 安全，仍是最优（建立临界靠近对手）
        assert_eq!(mv_wrap, Some((0, 2)), "wrap: expected (0,2), got {:?}", mv_wrap);
        // default 模式下 (4,2) 不在 (0,2) 邻域 → 无对手临近 → 仍是 (0,2)
        let mv_def = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Default, CapMode::Cap3);
        assert_eq!(mv_def, Some((0, 2)), "default: expected (0,2), got {:?}", mv_def);
    }
    #[test]
    fn strategy_wrap_detects_wrapped_threat() {
        let sz = 5;
        let mut b = mk_b(sz);
        // 己方临界 count2 在 (0,2)，对手临界 count2 在 (4,2)：wrap 下二者相邻 → 应引爆
        set(&mut b, 0, 2, 0, 2);
        set(&mut b, 4, 2, 1, 2);
        let mv = find_best_move_strategy(&b, sz, 0, &[], 2, 0, None, BorderMode::Wrap, CapMode::Cap3);
        assert_eq!(mv, Some((0, 2)), "wrap: wrapped adjacency should trigger explosion, got {:?}", mv);
    }

    // ── 15) MCTS 在 cap5 下能够执行引爆走法（count==4 合法） ──
    #[test]
    fn mcts_cap5_can_explode() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4);
        set(&mut b, 2, 3, 1, 4);
        let mv = find_best_move_mcts(&b, sz, 0, 2, &[], 2, BorderMode::Default, CapMode::Cap5);
        assert_eq!(mv, Some((2, 2)), "cap5 MCTS should pick explosive move, got {:?}", mv);
    }

    // ── 16) PVS 在 cap5 下能够执行引爆走法 ──
    #[test]
    fn pvs_cap5_can_explode() {
        let sz = 5;
        let mut b = mk_b(sz);
        set(&mut b, 2, 2, 0, 4);
        set(&mut b, 2, 3, 1, 4);
        let mv = find_best_move_pvs(&b, sz, 0, 2, &[], 2, 0, true, BorderMode::Default, CapMode::Cap5, 0);
        assert_eq!(mv, Some((2, 2)), "cap5 PVS should pick explosive move, got {:?}", mv);
    }

    // ── 17) Alpha-Beta 在 cap5 下能够执行引爆走法 ──
    // ── MCTS 性能基准（手动运行：cargo test --release mcts_bench -- --ignored --nocapture） ──
    #[test]
    #[ignore]
    fn mcts_bench() {
        use std::time::Instant;
        let sz = 7;
        for cm in [CapMode::Cap3, CapMode::Cap4, CapMode::Cap5] {
            let mut b = mk_b(sz);
            for i in 1..6 { for j in 1..6 {
                if (i + j) % 2 == 0 { set(&mut b, i, j, (i % 2) as usize, 2); }
            }}
            for depth in [1usize, 2, 3] {
                let t0 = Instant::now();
                let mv = find_best_move_mcts(&b, sz, 0, depth, &[], 2, BorderMode::Default, cm);
                let dt = t0.elapsed();
                println!("MCTS {:?} depth={} → {:?}  耗时 {:?}", cm, depth, mv, dt);
            }
        }
    }

    // ── 18) 新 18 维训练模型与 Rust 端加载兼容（冒烟测试产物存在时验证） ──
    #[test]
    fn xgb_v2_model_loadable() {
        let path = "/tmp/xgb_smoke.json";
        if std::path::Path::new(path).exists() {
            let eng = get_xgb_engine_for_test(path);
            let feats = [0.0f32; FEAT_DIM];
            let (raw, prob) = eng.predict(&feats);
            assert!(raw.is_finite(), "raw={raw}");
            assert!(prob.is_finite() && prob > 0.0 && prob < 1.0, "prob={prob}");
        }
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

    // ── 设备性能检测：首子布局合法性 ──
    #[test]
    fn bench_spread_starts_valid() {
        for &(sz, n) in &[(7usize, 4usize), (9, 6), (11, 6), (5, 2)] {
            let pos = spread_starts(sz, n);
            assert_eq!(pos.len(), n, "{sz}×{sz} {n} 人首子数量");
            for &(x, y) in &pos {
                assert!(x < sz && y < sz, "首子在界内");
            }
            for i in 0..n {
                for j in (i + 1)..n {
                    let d = pos[i].0.abs_diff(pos[j].0).max(pos[i].1.abs_diff(pos[j].1));
                    assert!(d >= 3, "{sz}×{sz}: 首子 ({:?}) 与 ({:?}) 距离 {d} < 3", pos[i], pos[j]);
                }
            }
        }
    }
}