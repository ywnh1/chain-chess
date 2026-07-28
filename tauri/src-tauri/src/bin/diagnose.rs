// Rust XGBoost 诊断：追踪树决策路径
use chain_chess_lib::{get_xgb_engine_for_test, xgb_predict};

fn main() {
    let engine = get_xgb_engine_for_test(
        "/home/ywnh1/Programs/chain-chess/tauri/src-tauri/xgb_model_board.json"
    );

    let feats = [0.0f32; 29];
    let (raw, prob) = xgb_predict(&engine, &feats);
    println!("全零特征: raw={:.6} prob={:.6}", raw, prob);

    // 追踪第1棵树
    println!("\n第1棵树路径 (全零特征):");
    let t = &engine.trees[0];
    let mut idx = 0i32;
    let mut depth = 0;
    while depth < 20 {
        let n = &t[idx as usize];
        if n.is_leaf {
            println!("  [depth={}] node={} LEAF value={:.6}", depth, idx, n.leaf);
            break;
        }
        let go_left = feats[n.split_feat] <= n.split_cond;
        let child = if go_left { n.left } else { n.right };
        println!("  [depth={}] node={} feat[{}]={} <= {:.4}? {} -> {}", depth, idx,
            n.split_feat, feats[n.split_feat], n.split_cond, go_left, child);
        idx = child;
        depth += 1;
    }

    // 计算所有树的累积值
    let mut sum = 0.0f32;
    for t in &engine.trees {
        let mut i = 0i32;
        loop {
            let n = &t[i as usize];
            if n.is_leaf { sum += n.leaf; break; }
            i = if feats[n.split_feat] <= n.split_cond { n.left } else { n.right };
        }
    }
    let base_score = 0.42333055f32;
    let raw2 = sum + base_score;
    let prob2 = 1.0 / (1.0 + (-raw2).exp());
    println!("\n对比:");
    println!("  Python: raw=unknown prob=0.018144");
    println!("  Rust:   sum={:.6} base_score={:.6} raw={:.6} prob={:.6}", sum, base_score, raw2, prob2);
}
