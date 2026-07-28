# 连锁棋 XGBoost AI 训练与集成状态

## ✅ 已完成

### 数据生成 (Rust)
- [x] Rust `generate_selfplay_data()` 函数 (lib.rs 末尾)
- [x] CLI 二进制 `selfplay` (`cargo run --release --bin selfplay`)
- [x] 支持多 AI 混合配置 (strategy/alphabeta/pvs/mcts)

### 数据合并 (Python)
- [x] `data/merge_data.py` — 合并多份 JSONL，重新分配 game_id，跳过未完成游戏
- [x] 混合配置 (7×7/12×12/9×9 + 2p/4p) 自动处理

### 训练 (Python)
- [x] `data/train_xgb.py` — 完整训练管线
- [x] 训练两个模型：
  - **走法评估模型** (37维) → `xgb_model_fast.json` ✅ 已生成
  - **盘面评估模型** (29维) → `xgb_model_board.json` ❌ 明天跑

### Rust 集成
- [x] 纯 Rust XGBoost 推理引擎 (零外部依赖)
- [x] 29 维特征提取 (`extract_features_xgb`)
- [x] `eval_board_ml()` 函数
- [x] 替换所有 `eval_board()` 调用点
- [x] 编译通过 (`cargo check`)

## ❌ 明天要做

### 1. 训练两个模型
```bash
cd ~/Programs/chain-chess
python3 data/train_xgb.py --input ./data/selfplay_merged.jsonl --max-players 4
```
会生成两个文件：
- `xgb_model.json` (走法评估 37维) ✅
- **`xgb_model_board.json`** (盘面评估 29维) ← Rust 引擎用

### 2. 复制模型到引擎目录
```bash
cp ./xgb_model_board.json ./tauri/src-tauri/
```

### 3. 构建测试
```bash
cd ~/Programs/chain-chess/tauri/src-tauri
cargo build --release
```

### 切换评估函数
在游戏 **AI 对战** 设置页面，新增了「AI 评估函数」切换开关：
- **ML 模型**（默认）— XGBoost 盘面评估
- **手写规则** — 原始的启发式评估

可以在开始游戏前随时切换，不影响其他设置。
