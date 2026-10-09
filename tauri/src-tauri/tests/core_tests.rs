use chain_chess_core::*;

#[test]
fn test_find_best_move_finds_valid_move() {
    // Empty board with one piece for player 0
    let mut b: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None }; 7]; 7];
    b[3][3] = Cell { owner: Some(0), count: 1, th: None };
    let result = find_best_move(&b, 7, 0, 1, &[], 2, 10, None, false, BorderMode::Default, CapMode::Cap4, 0);
    assert!(result.is_some(), "AI should find a valid move at depth=1");
    let (x, y) = result.unwrap();
    assert!(x < 7 && y < 7);
    // AI should pick a cell owned by player 0
    assert_eq!(b[x][y].owner, Some(0), "AI should pick its own piece");
}

#[test]
fn test_find_best_move_strategy_returns_something() {
    let mut b: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None }; 5]; 5];
    b[2][2] = Cell { owner: Some(0), count: 3, th: None };
    let result = find_best_move_strategy(&b, 5, 0, &[], 2, 10, None, BorderMode::Default, CapMode::Cap4);
    assert!(result.is_some(), "Strategy AI should find a move");
    let (x, y) = result.unwrap();
    assert!(x < 5 && y < 5);
}

#[test]
fn test_cell_default_state() {
    let cell = Cell { owner: None, count: 0, th: None };
    assert_eq!(cell.owner, None);
    assert_eq!(cell.count, 0);
}

#[test]
fn test_cell_occupied() {
    let cell = Cell { owner: Some(0), count: 3, th: None };
    assert_eq!(cell.owner, Some(0));
    assert_eq!(cell.count, 3);
}

#[test]
fn test_process_move_result_defaults() {
    let b: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None }; 5]; 5];
    let result = ProcessMoveResult {
        board: b.clone(),
        eliminated: vec![],
        killed_by: vec![],
        chain_count: 0,
        game_over: false,
        winner: None,
        steps: vec![],
        exploded: vec![],
    };
    assert!(!result.game_over);
    assert!(result.winner.is_none());
    assert_eq!(result.chain_count, 0);
    assert!(result.eliminated.is_empty());
    assert_eq!(result.board.len(), 5);
}

#[test]
fn test_player_ai_config_defaults() {
    use serde_json;
    let json = r#"{"algorithm":"alphabeta","depth":2}"#;
    let config: PlayerAiConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.algorithm, "alphabeta");
    assert_eq!(config.depth, 2);
}

#[test]
fn test_player_ai_config_with_ml_eval() {
    use serde_json;
    let json = r#"{"algorithm":"pvs","depth":3,"use_ml_eval":true}"#;
    let config: PlayerAiConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.algorithm, "pvs");
    assert_eq!(config.depth, 3);
    assert!(config.use_ml_eval);
}

#[test]
fn test_history_record_serde() {
    use serde_json;
    let record = HistoryRecord {
        id: 1,
        time: "2026-07-28 12:00".to_string(),
        mode: "eve".to_string(),
        ai_algorithm: "alphabeta/d2,mcts/d3".to_string(),
        ai_depth: 2,
        game_count: 1,
        player_count: 3,
        ai_count: 3,
        board_size: 9,
        border_mode: Some("default".to_string()),
        cap_mode: Some("4".to_string()),
        winner: Some(1),
        color_names: vec!["红色".to_string(), "黄色".to_string(), "蓝色".to_string()],
        chain_stats: std::collections::HashMap::new(),
        max_chain: MaxChain { player: Some(0), length: 5 },
        history: serde_json::Value::Null,
        finished: Some(true),
        game_state: None,
    };
    let json = serde_json::to_string(&record).unwrap();
    let parsed: HistoryRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, 1);
    assert_eq!(parsed.ai_algorithm, "alphabeta/d2,mcts/d3");
    assert_eq!(parsed.winner, Some(1));
}

#[test]
fn test_process_click_with_killer_identifies_eliminator() {
    // 3x3: 玩家1 在 (1,1) 只有 1 颗棋子；玩家0 在 (2,1) 有 3 颗，落子后爆裂吞掉玩家1
    let mut b: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None }; 3]; 3];
    b[1][1] = Cell { owner: Some(1), count: 1, th: None };
    b[2][1] = Cell { owner: Some(0), count: 3, th: None };
    let (eliminated, _chain, killed_by) = process_click_with_killer(&mut b, 3, 2, 1, 0, 2, BorderMode::Default, CapMode::Cap4, None);
    assert_eq!(eliminated, vec![1], "player 1 should be eliminated");
    assert_eq!(killed_by, vec![(1, 0)], "player 0 should be the killer of player 1");
    assert_eq!(b[1][1].owner, Some(0), "cell should now belong to player 0");
}

#[test]
fn test_process_click_wrapper_still_works() {
    // 简单版本应保持原行为（返回二元组）
    let mut b: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None }; 3]; 3];
    b[1][1] = Cell { owner: Some(1), count: 1, th: None };
    b[2][1] = Cell { owner: Some(0), count: 3, th: None };
    let (eliminated, chain) = process_click(&mut b, 3, 2, 1, 0, 2, BorderMode::Default, CapMode::Cap4, None);
    assert_eq!(eliminated, vec![1]);
    assert!(chain >= 1);
}

#[test]
fn test_chain_reaction_no_deadlock_degrade() {
    // 回归：degrade 模式此前在"角落密集互喂"局面下会无限连锁死锁
    // 修复：process_click 内部有连锁步数防御上限，必须能返回
    let sz = 5usize;
    let setup: [((usize, usize), usize, u8); 22] = [
        ((0, 0), 0, 1), ((0, 2), 0, 2), ((0, 3), 0, 2), ((0, 4), 0, 1),
        ((1, 0), 0, 2), ((1, 1), 0, 1), ((1, 2), 0, 3), ((1, 4), 0, 1),
        ((2, 0), 0, 2), ((2, 1), 0, 2), ((2, 2), 0, 1), ((2, 3), 0, 2), ((2, 4), 0, 2),
        ((3, 0), 0, 2), ((3, 1), 0, 3), ((3, 2), 0, 2), ((3, 3), 0, 1), ((3, 4), 0, 2),
        ((4, 1), 0, 2), ((4, 2), 0, 2), ((4, 3), 0, 2), ((4, 4), 1, 1),
    ];
    let mut b: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None }; sz]; sz];
    for ((i, j), o, c) in setup {
        b[i][j] = Cell { owner: Some(o), count: c, th: None };
    }
    // 修复前：此调用永不返回（死循环）
    let (elim, chain) = process_click(&mut b, sz, 4, 4, 1, 3, BorderMode::Degrade, CapMode::Cap4, None);
    assert!(
        chain < 1_000_000,
        "连锁步数异常（被防御截断仍应在合理上限内）: chain={}",
        chain
    );
    // 连锁后玩家1应存活（(4,4) 角落已爆，但防御截断后状态合法可继续）
    let _ = elim;
    assert!(has_pieces(&b, 1), "player 1 应仍有棋子（或已被淘汰，均不算死锁）");
}

#[test]
fn test_chain_reaction_wrap_no_deadlock() {
    // wrap 模式：拥挤棋盘长对局可能进入互喂周期，防御上限必须生效
    let sz = 7usize;
    let mut b: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None }; sz]; sz];
    // 构造整圈 3 子环绕（wrap 下每格都有 4 邻居，能量守恒）
    for i in 0..sz {
        b[i][0] = Cell { owner: Some(0), count: 3, th: None };
        b[i][sz - 1] = Cell { owner: Some(0), count: 3, th: None };
        b[0][i] = Cell { owner: Some(1), count: 3, th: None };
        b[sz - 1][i] = Cell { owner: Some(1), count: 3, th: None };
    }
    let (elim, chain) = process_click(&mut b, sz, 3, 3, 0, 2, BorderMode::Wrap, CapMode::Cap4, None);
    assert!(chain < 1_000_000, "wrap 连锁步数异常: chain={}", chain);
    let _ = elim;
}

#[test]
fn test_killed_by_all_border_modes() {
    // 4 种边界模式下，击败者（killed_by）识别均正确
    // 场景：玩家0 在 (2,1) 3 子落子引爆，吞掉玩家1 位于 (1,1) 的唯一棋子
    let modes = [
        ("default", BorderMode::Default),
        ("degrade", BorderMode::Degrade),
        ("wrap", BorderMode::Wrap),
        ("bounce", BorderMode::Bounce),
    ];
    for (name, m) in modes {
        let sz = 3usize;
        let mut b: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None }; sz]; sz];
        b[1][1] = Cell { owner: Some(1), count: 1, th: None };
        b[2][1] = Cell { owner: Some(0), count: 3, th: None };
        let (eliminated, _chain, killed_by) = process_click_with_killer(&mut b, sz, 2, 1, 0, 2, m, CapMode::Cap4, None);
        assert_eq!(eliminated, vec![1], "[{}] player 1 应被淘汰", name);
        assert_eq!(killed_by, vec![(1, 0)], "[{}] 击败者应为玩家0", name);
        assert_eq!(b[1][1].owner, Some(0), "[{}] (1,1) 应归玩家0", name);
    }
}

#[test]
fn test_killed_by_degrade_corner() {
    // degrade 模式：角上 cap=2，玩家0 落角上引爆吞掉玩家1
    let sz = 3usize;
    let mut b: GameBoard = vec![vec![Cell { owner: None, count: 0, th: None }; sz]; sz];
    b[0][1] = Cell { owner: Some(1), count: 1, th: None };
    b[0][0] = Cell { owner: Some(0), count: 1, th: None };
    // 玩家0 落子 (0,0)：count 1->2 达角上阈值 2 引爆，向 (0,1) 扩散吞掉玩家1
    let (eliminated, _chain, killed_by) = process_click_with_killer(&mut b, sz, 0, 0, 0, 2, BorderMode::Degrade, CapMode::Cap4, None);
    assert_eq!(eliminated, vec![1], "degrade 角上玩家1 应被淘汰");
    assert_eq!(killed_by, vec![(1, 0)], "degrade 击败者应为玩家0");
}

// ─── 异型棋盘：长方形 ───

/// 造一个 rows × cols 的空棋盘
fn rect_board(rows: usize, cols: usize) -> GameBoard {
    vec![vec![Cell { owner: None, count: 0, th: None }; cols]; rows]
}

#[test]
fn test_rect_board_keeps_its_dimensions() {
    let b = rect_board(7, 5);
    assert_eq!(b.len(), 7, "行数");
    assert_eq!(b[0].len(), 5, "列数");
}

/// 邻居要按 rows/cols 分别判断边界：以前只有一个 sz，长方棋盘会在短边越界
#[test]
fn test_rect_neighbors_respect_rows_and_cols() {
    assert_eq!(nbrs(0, 0, 7, 5).len(), 2, "左上角只有两个邻居");
    assert_eq!(nbrs(6, 4, 7, 5).len(), 2, "右下角只有两个邻居");
    assert_eq!(nbrs(6, 2, 7, 5).len(), 3, "最后一行中间（行边界）");
    assert_eq!(nbrs(2, 4, 7, 5).len(), 3, "最后一列中间（列边界）");
    assert_eq!(nbrs(3, 2, 7, 5).len(), 4, "内部四邻居");
    // 关键回归：若列边界错用行数(7)，第 5 列会越界
    for i in 0..7 {
        for j in 0..5 {
            let ns = nbrs(i, j, 7, 5);
            assert!(ns.iter().all(|&(a, b)| a < 7 && b < 5), "({},{}) 的邻居越界: {:?}", i, j, ns);
        }
    }
}

#[test]
fn test_rect_get_moves_stay_inside() {
    let b = rect_board(6, 4);
    let m = get_moves(&b, 6, 0, None, BorderMode::Default, CapMode::Cap4);
    assert!(!m.is_empty(), "空棋盘应当有走法");
    assert!(m.iter().all(|&(i, j)| i < 6 && j < 4), "走法必须在棋盘内: {:?}", m);
}

/// 7×5 长方形跑完整局：不 panic、棋盘尺寸不变、胜者合法
#[test]
fn test_rect_full_game_runs_to_end() {
    let mut b = rect_board(7, 5);
    b[1][1] = Cell { owner: Some(0), count: 3, th: None };
    b[5][3] = Cell { owner: Some(1), count: 3, th: None };
    let res = simulate_to_end(
        b, 7, 2, 0, vec![], BorderMode::Default, CapMode::Cap4, None, 10,
        std::collections::HashMap::new(),
    );
    assert_eq!(res.board.len(), 7);
    assert_eq!(res.board[0].len(), 5);
    if let Some(w) = res.winner {
        assert!(w < 2, "胜者必须是参战玩家，实际 {}", w);
    }
}

/// 回环边界在长方形上要按各自的边长回绕
#[test]
fn test_rect_wrap_uses_cols_for_columns() {
    let ns = nbrs_wrap(0, 0, 7, 5);
    assert!(ns.contains(&(6, 0)), "上行回绕到最后一行(6)");
    assert!(ns.contains(&(0, 4)), "左列回绕到最后一列(4)");
    assert!(ns.iter().all(|&(i, j)| i < 7 && j < 5), "回绕结果必须仍在棋盘内: {:?}", ns);
}
