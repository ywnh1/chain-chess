#!/usr/bin/env python3
"""
连锁棋 XGBoost 训练管线
=======================
步骤:
  1. 从 JSONL 加载原始对弈数据
  2. 特征工程：从棋盘状态提取 ~20 个特征
  3. 标签化：该走法是否最终获胜
  4. 训练 XGBoost 回归模型
  5. 评估并导出模型

用法:
  ./data/train_xgb.py                  # 使用默认路径
  ./data/train_xgb.py --input <path>   # 指定输入文件
"""

import json
import sys
import os
import argparse
from collections import defaultdict

import numpy as np
import xgboost as xgb
from sklearn.model_selection import train_test_split
from sklearn.metrics import accuracy_score, roc_auc_score

# ============================================================
# 1. 加载数据
# ============================================================

def load_jsonl(path: str):
    """加载 JSONL 文件，按 game_id 分组，跳过损坏行和未完成游戏"""
    games = defaultdict(list)
    bad_lines = 0
    with open(path, "r", encoding="utf-8") as f:
        for line_no, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            try:
                d = json.loads(line)
                games[d["game_id"]].append(d)
            except json.JSONDecodeError as e:
                bad_lines += 1
                if bad_lines <= 3:
                    print(f"  ⚠️  第{line_no}行 JSON 解析失败: {e}")

    if bad_lines:
        print(f"  ⚠️  共跳过 {bad_lines} 行损坏数据")

    valid_games = {}
    for gid, steps in games.items():
        if steps[-1].get("game_over", False):
            valid_games[gid] = steps
    return valid_games


# ============================================================
# 2. 特征工程
# ============================================================

def extract_features(board, cur_player, max_players):
    """
    从棋盘状态提取特征向量。
    特征定义与 Rust eval_board() 对齐，确保训练出的模型可以无缝替换。

    返回: numpy array, 长度 = 6 + 6*players + 4 = 10 + 6*players
    """
    size = len(board)
    feats = []

    # ---- 全局特征 ----
    total_pieces = sum(1 for row in board for c in row if c["owner"] is not None)
    feats.append(total_pieces / (size * size))  # 棋盘填充率

    center = (size - 1) / 2.0

    # ---- 每个玩家的特征 ----
    for p in range(max_players):
        p_count_1 = 0  # count=1 棋子数
        p_count_2 = 0
        p_count_3 = 0
        p_territory = 0  # 占领格子数
        p_chain_threat = 0  # 连锁威胁（count=3 加权）
        p_center_dist = 0.0  # 距棋盘中心距离（归一化）

        for i in range(size):
            for j in range(size):
                c = board[i][j]
                if c["owner"] == p:
                    cnt = c["count"]
                    p_territory += 1
                    if cnt == 1:
                        p_count_1 += 1
                    elif cnt == 2:
                        p_count_2 += 1
                    elif cnt == 3:
                        p_count_3 += 1
                        p_chain_threat += cnt * 4
                    elif cnt == 2:
                        p_chain_threat += 1
                    # 到中心的距离
                    dist = abs(i - center) + abs(j - center)
                    p_center_dist += dist

        # 归一化
        max_dist = size * 2
        feats.append(p_count_1 / max(1, total_pieces))
        feats.append(p_count_2 / max(1, total_pieces))
        feats.append(p_count_3 / max(1, total_pieces))
        feats.append(p_territory / (size * size))
        feats.append(p_chain_threat / max(1, total_pieces * 4))
        feats.append(p_center_dist / (max_dist * max(1, p_territory)))

    # ---- 当前玩家视角特征 ----
    # 当前玩家棋子占比
    cur_total = sum(1 for row in board for c in row if c["owner"] == cur_player)
    feats.append(cur_total / max(1, total_pieces))
    # 当前玩家 count=3 占比
    cur_lv3 = sum(1 for row in board for c in row
                  if c["owner"] == cur_player and c["count"] == 3)
    feats.append(cur_lv3 / max(1, cur_total))
    # 当前玩家领地对所有领地的占比
    feats.append(cur_total / max(1, size * size))
    # 游戏进程（填充率）
    feats.append(total_pieces / (size * size))

    return np.array(feats, dtype=np.float32)


def extract_move_features(board, cur_player, max_players, mx, my):
    """
    针对当前选择的走法提取特征。
    这些特征结合了"全局局面"和"这个具体走法的上下文"。
    """
    size = len(board)
    feats = []

    # 目标格子的信息
    cell = board[mx][my]
    feats.append(cell["count"] / 3.0)  # 当前等级 (0~3)

    # 周围的对手棋子（四种邻居方向）
    opp_nearby = 0
    opp_lv3_nearby = 0
    for di, dj in [(-1, 0), (1, 0), (0, -1), (0, 1)]:
        ni, nj = mx + di, my + dj
        if 0 <= ni < size and 0 <= nj < size:
            nc = board[ni][nj]
            if nc["owner"] is not None and nc["owner"] != cur_player:
                opp_nearby += 1
                if nc["count"] == 3:
                    opp_lv3_nearby += 1
    feats.append(opp_nearby / 4.0)
    feats.append(opp_lv3_nearby / 4.0)

    # 周围的己方棋子
    ally_nearby = sum(
        1 for di, dj in [(-1, 0), (1, 0), (0, -1), (0, 1)]
        if 0 <= mx + di < size and 0 <= my + dj < size
        and board[mx + di][my + dj]["owner"] == cur_player
    )
    feats.append(ally_nearby / 4.0)

    # 是否边/角
    is_edge = 1.0 if mx == 0 or mx == size - 1 or my == 0 or my == size - 1 else 0.0
    is_corner = 1.0 if (mx == 0 or mx == size - 1) and (my == 0 or my == size - 1) else 0.0
    feats.append(is_edge)
    feats.append(is_corner)

    # 距中心距离
    center = (size - 1) / 2.0
    dist_to_center = abs(mx - center) + abs(my - center)
    feats.append(dist_to_center / (size * 2))

    # 落子后 count 是否会达到 3（下一手就炸）
    will_be_3 = 1.0 if (cell["owner"] == cur_player and cell["count"] == 2) else 0.0
    feats.append(will_be_3)

    return np.array(feats, dtype=np.float32)


def prepare_dataset(games, max_players, size, include_move=True):
    """
    遍历所有游戏，生成 (X, y) 数据集。

    include_move=True:  每个样本 = 全局特征 + 走法特征 (37维)  → 走法评估模型
    include_move=False: 每个样本 = 全局特征 (29维)            → 盘面评估模型

    标签 y = 1.0 如果 cur_player == winner, 否则 0.0
    """
    X_list = []
    y_list = []

    for gid, steps in games.items():
        winner = steps[-1]["winner"]

        for step in steps:
            if step["game_over"]:
                continue

            board = step["board"]
            cur_player = step["cur_player"]
            global_feats = extract_features(board, cur_player, max_players)

            if include_move:
                mx, my = step["move_x"], step["move_y"]
                move_feats = extract_move_features(board, cur_player, max_players, mx, my)
                X = np.concatenate([global_feats, move_feats])
            else:
                X = global_feats

            y = 1.0 if winner == cur_player else 0.0
            X_list.append(X)
            y_list.append(y)

    return np.array(X_list, dtype=np.float32), np.array(y_list, dtype=np.float32)


# ============================================================
# 3. 训练
# ============================================================

def train(X, y):
    """训练 XGBoost 回归模型"""
    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.15, random_state=42
    )

    print(f"  训练样本: {len(X_train)}")
    print(f"  测试样本: {len(X_test)}")
    print(f"  特征维度: {X.shape[1]}")

    # ---- XGBoost ----
    # 目标是预测胜率 (0~1)，用回归 + logistic 目标
    model = xgb.XGBRegressor(
        n_estimators=500,
        max_depth=6,
        learning_rate=0.05,
        subsample=0.8,
        colsample_bytree=0.8,
        reg_lambda=1.0,
        reg_alpha=0.1,
        objective="reg:logistic",
        random_state=42,
        n_jobs=-1,
        verbosity=1,
    )

    # 训练
    model.fit(
        X_train, y_train,
        eval_set=[(X_test, y_test)],
        verbose=50,
    )

    # ---- 评估 ----
    y_pred = model.predict(X_test)
    y_pred_bin = (y_pred > 0.5).astype(float)

    acc = accuracy_score(y_test, y_pred_bin)
    try:
        auc = roc_auc_score(y_test, y_pred)
    except ValueError:
        auc = 0.0

    print(f"\n  测试集准确率: {acc:.4f}")
    print(f"  测试集 AUC:    {auc:.4f}")

    # ---- 特征重要性 ----
    importance = model.feature_importances_
    top_k = min(10, len(importance))
    top_idx = np.argsort(importance)[::-1][:top_k]
    print(f"\n  前 {top_k} 个重要特征:")
    for rank, idx in enumerate(top_idx, 1):
        print(f"    {rank}. 特征 {idx}: {importance[idx]:.4f}")

    return model


# ============================================================
# 4. 主流程
# ============================================================

def main():
    parser = argparse.ArgumentParser(description="连锁棋 XGBoost 训练")
    parser.add_argument("--input", default=None,
                        help="JSONL 输入文件路径")
    parser.add_argument("--output", default="./xgb_model.json",
                        help="模型输出路径 (XGBoost JSON format)")
    parser.add_argument("--max-players", type=int, default=4,
                        help="最大玩家数")
    parser.add_argument("--board-size", type=int, default=9,
                        help="棋盘大小")
    args = parser.parse_args()

    # 默认输入路径
    input_path = args.input
    if input_path is None:
        cwd = os.getcwd()
        script_dir = os.path.dirname(os.path.abspath(__file__))
        candidates = [
            os.path.join(cwd, "data", "selfplay_merged.jsonl"),
            os.path.join(cwd, "selfplay_merged.jsonl"),
            os.path.join(cwd, "tauri", "src-tauri", "data"),
            os.path.join(script_dir, "..", "tauri", "src-tauri", "data"),
        ]
        input_path = None
        for c in candidates:
            if os.path.isfile(c) and c.endswith(".jsonl"):
                input_path = c
                break
            if os.path.isdir(c):
                jsonls = sorted([f for f in os.listdir(c)
                                 if f.endswith(".jsonl") and os.path.getsize(os.path.join(c, f)) > 1000])
                if jsonls:
                    input_path = os.path.join(c, jsonls[-1])
                    break
        if input_path is None:
            print("❌ 未找到 JSONL 文件，请用 --input 指定路径")
            sys.exit(1)
        print(f"📂 自动选择: {input_path}")

    print("=" * 55)
    print("  连锁棋 XGBoost 训练管线")
    print("=" * 55)

    # 1. 加载
    print("\n[1/4] 加载数据...")
    games = load_jsonl(input_path)
    print(f"  ✅ 加载 {len(games)} 局, {sum(len(v) for v in games.values())} 步")

    # 2. 特征工程
    print("\n[2/4] 特征提取...")
    X, y = prepare_dataset(games, args.max_players, args.board_size)
    print(f"  ✅ 特征矩阵: {X.shape}")
    print(f"  ✅ 正样本率: {y.mean():.3f}")

    # 3. 训练
    print("\n[3/4] 训练 XGBoost...")
    model = train(X, y)

    # 4. 导出
    print("\n[4/4] 导出模型...")
    model.save_model(args.output)
    print(f"  ✅ 模型已保存: {args.output}")
    print(f"     大小: {os.path.getsize(args.output) / 1024:.1f} KB")

    print("\n" + "=" * 55)
    print("  ✅ 走法评估模型完成！")
    print("=" * 55)

    board_output = args.output.replace(".json", "_board.json")
    print(f"\n[额外] 训练盘面评估模型 (29维)...")
    print(f"  输出: {board_output}")
    Xb, yb = prepare_dataset(games, args.max_players, args.board_size, include_move=False)
    print(f"  特征矩阵: {Xb.shape}")
    model_b = train(Xb, yb)
    model_b.save_model(board_output)
    print(f"  ✅ 盘面评估模型已保存: {board_output}")
    print(f"     大小: {os.path.getsize(board_output) / 1024:.1f} KB")

    print("\n" + "=" * 55)
    print("  ✅ 全部训练完成！")
    print("=" * 55)
    print(f"\n  走法评估模型 (37维): {args.output}")
    print(f"  盘面评估模型 (29维): {board_output}")
    print(f"\n下一步: 将 {board_output} 集成到 Rust 引擎中替换 eval_board()")


if __name__ == "__main__":
    main()
