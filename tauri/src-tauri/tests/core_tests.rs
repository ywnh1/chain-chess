use chain_chess_lib::*;

#[test]
fn test_find_best_move_finds_valid_move() {
    // Empty board with one piece for player 0
    let mut b: GameBoard = vec![vec![Cell { owner: None, count: 0 }; 7]; 7];
    b[3][3] = Cell { owner: Some(0), count: 1 };
    let result = find_best_move(&b, 7, 0, 1, &[], 2, 10, None, false);
    assert!(result.is_some(), "AI should find a valid move at depth=1");
    let (x, y) = result.unwrap();
    assert!(x < 7 && y < 7);
    // AI should pick a cell owned by player 0
    assert_eq!(b[x][y].owner, Some(0), "AI should pick its own piece");
}

#[test]
fn test_find_best_move_strategy_returns_something() {
    let mut b: GameBoard = vec![vec![Cell { owner: None, count: 0 }; 5]; 5];
    b[2][2] = Cell { owner: Some(0), count: 3 };
    let result = find_best_move_strategy(&b, 5, 0, &[], 2, 10, None);
    assert!(result.is_some(), "Strategy AI should find a move");
    let (x, y) = result.unwrap();
    assert!(x < 5 && y < 5);
}

#[test]
fn test_cell_default_state() {
    let cell = Cell { owner: None, count: 0 };
    assert_eq!(cell.owner, None);
    assert_eq!(cell.count, 0);
}

#[test]
fn test_cell_occupied() {
    let cell = Cell { owner: Some(0), count: 3 };
    assert_eq!(cell.owner, Some(0));
    assert_eq!(cell.count, 3);
}

#[test]
fn test_process_move_result_defaults() {
    let b: GameBoard = vec![vec![Cell { owner: None, count: 0 }; 5]; 5];
    let result = ProcessMoveResult {
        board: b.clone(),
        eliminated: vec![],
        chain_count: 0,
        game_over: false,
        winner: None,
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
