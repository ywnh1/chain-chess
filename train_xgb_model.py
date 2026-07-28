"""
连锁棋 ML 评估模型训练脚本
方案一：用 eval_board_improved 中间值做特征

数据格式（每行一条 JSON）:
  {"board":[[{"owner":0,"count":1},...],...], "max_players":2,
   "cur_player":0, "winner":0, "turn":42, "size":7}

特征（16 维 — 与 Rust 端 extract_features_improved 同步）:
  [0]  density — 棋盘密度 (0~1)
  [1]  alive_ratio — 存活比例 (0~1)
  [2]  my_share — 我的棋子占总棋子比 (0~1)
  [3]  c3_share — 三级棋子占我的棋子比 (0~1)
  [4]  c2_share — 二级棋子占我的棋子比 (0~1)
  [5]  territory_share — 我的地盘占棋盘比 (0~1)
  [6]  score_diff — 分数差 / 棋盘面积
  [7]  chain_threat_diff — 爆发势能差 / 棋盘面积
  [8]  pos_bonus_diff — 位置优势差 / 棋盘面积
  [9]  threat_prox_diff — 威胁逼近差 / 棋盘面积
  [10] my_score_norm — 我的分数 / (对手分数+1)
  [11] opp_score_norm — 对手分数 / (我的分数+1) (截断)
  [12] center_dist_balance — 距离中心差值 / 棋盘半径
  [13] territory_balance — 地盘差 / 棋盘面积
  [14] my_threat_ratio — 我的三级棋子威胁 / 我的棋子数
  [15] opp_threat_ratio — 对手三级棋子威胁 / 对手棋子数
"""

import json
import sys
import numpy as np
import xgboost as xgb
from sklearn.model_selection import train_test_split
from sklearn.metrics import accuracy_score, roc_auc_score

# 尝试导入 tqdm（可选）
try:
    from tqdm import tqdm
except ImportError:
    def tqdm(iterable, **kwargs):
        return iterable

# ─── 特征提取 ───

def extract_features(board, cur_player, max_players, size):
    """提取16维特征向量，与 Rust extract_features_improved 一致"""
    total_cells = size * size
    total_pieces = sum(1 for row in board for c in row if c['owner'] is not None)

    # 统计每个玩家的数据
    my_score = 0; opp_score = 0
    my_territory = 0; opp_territory = 0
    my_chain_threat = 0; opp_chain_threat = 0
    my_pos_bonus = 0; opp_pos_bonus = 0
    my_threat_prox = 0; opp_threat_prox = 0
    my_pieces = 0; opp_pieces = 0
    my_c2 = 0; my_c3 = 0
    opp_c2 = 0; opp_c3 = 0
    my_cdist = 0.0; opp_cdist = 0.0
    alive_count = 0

    cx = (size - 1) * 0.5

    for p in range(max_players):
        has_pieces = False
        for i in range(size):
            for j in range(size):
                c = board[i][j]
                if c['owner'] == p:
                    has_pieces = True
                    cnt = c['count']
                    d_val = 4 - int(abs(i - cx) * 0.5 + abs(j - cx) * 0.5)
                    pos_val = max(d_val, 0)

                    if p == cur_player:
                        my_score += cnt
                        my_territory += 1
                        my_pos_bonus += pos_val
                        my_cdist += abs(i - cx) + abs(j - cx)
                        my_pieces += 1
                        if cnt == 2:
                            my_c2 += 1
                        elif cnt == 3:
                            my_c3 += 1
                            my_chain_threat += cnt * 5
                        elif cnt >= 2:
                            my_chain_threat += 2
                        # 邻居对手计数
                        for ni, nj in nbrs(i, j, size):
                            nc = board[ni][nj]
                            if nc['owner'] is not None and nc['owner'] != p:
                                my_threat_prox += cnt
                    else:
                        opp_score += cnt
                        opp_territory += 1
                        opp_pos_bonus += pos_val
                        opp_cdist += abs(i - cx) + abs(j - cx)
                        opp_pieces += 1
                        if cnt == 2:
                            opp_c2 += 1
                        elif cnt == 3:
                            opp_c3 += 1
                            opp_chain_threat += cnt * 5
                        elif cnt >= 2:
                            opp_chain_threat += 2
                        for ni, nj in nbrs(i, j, size):
                            nc = board[ni][nj]
                            if nc['owner'] == cur_player:
                                opp_threat_prox += cnt
        if has_pieces:
            alive_count += 1

    total = max(total_pieces, 1)
    sz = float(size)

    feats = np.zeros(16, dtype=np.float32)

    feats[0] = total_pieces / total_cells              # density
    feats[1] = alive_count / max_players                # alive_ratio
    feats[2] = my_pieces / total                        # my_share
    feats[3] = my_c3 / max(my_pieces, 1)                # c3_share
    feats[4] = my_c2 / max(my_pieces, 1)                # c2_share
    feats[5] = my_territory / total_cells               # territory_share
    feats[6] = (my_score - opp_score) / total_cells     # score_diff
    feats[7] = (my_chain_threat - opp_chain_threat) / total_cells  # chain_threat_diff
    feats[8] = (my_pos_bonus - opp_pos_bonus) / total_cells        # pos_bonus_diff
    feats[9] = (my_threat_prox - opp_threat_prox) / total_cells    # threat_prox_diff
    feats[10] = my_score / max(opp_score, 1)            # my_score_norm
    feats[11] = opp_score / max(my_score, 1)            # opp_score_norm
    feats[12] = (my_cdist / max(my_pieces, 1) - opp_cdist / max(opp_pieces, 1)) / (sz * 0.5)  # center_dist_balance
    feats[13] = (my_territory - opp_territory) / total_cells       # territory_balance
    feats[14] = my_c3 / max(my_pieces, 1)               # my_threat_ratio (same as c3_share)
    feats[15] = opp_c3 / max(opp_pieces, 1)             # opp_threat_ratio

    return feats


def nbrs(i, j, sz):
    """返回四邻域坐标"""
    result = []
    if i > 0: result.append((i-1, j))
    if i + 1 < sz: result.append((i+1, j))
    if j > 0: result.append((i, j-1))
    if j + 1 < sz: result.append((i, j+1))
    return result


def max(a, b):
    return a if a > b else b


# ─── 数据加载 ───

def count_lines(path):
    """快速统计 JSONL 行数"""
    with open(path, 'rb') as f:
        return sum(1 for _ in f)

def load_selfplay_data(data_paths):
    """从 selfplay JSONL 文件加载数据
    策略：遍历每个游戏，收集所有步
    当遇到 game_over=true 时，知道该局胜者
    用胜者标记该局所有步的标签（当前玩家是否最终获胜）
    """
    features = []
    labels = []

    for path in data_paths:
        total_lines = count_lines(path)
        valid = 0
        skipped = 0
        games_found = 0

        # 缓冲当前游戏的所有记录
        buf = []
        current_game = None

        with open(path) as f:
            for line in tqdm(f, total=total_lines, desc=f'  解析 {path.split("/")[-1]}', unit='行', leave=False):
                line = line.strip()
                if not line:
                    skipped += 1
                    continue
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    skipped += 1
                    continue

                game_id = rec['game_id']
                board = rec['board']
                total_pieces = sum(1 for row in board for c in row if c['owner'] is not None)

                # 跳过空棋盘（第一手）
                if total_pieces == 0:
                    skipped += 1
                    continue

                # 新游戏开始 → 丢弃上一局未结束的缓冲
                if current_game is not None and game_id != current_game:
                    buf = []

                current_game = game_id
                game_over = rec.get('game_over', False)
                winner = rec.get('winner')
                cur_player = rec['cur_player']
                max_players = rec['max_players']
                size = rec['size']

                # 即使是终局步也加入缓冲（知道 winner 后统一标记）
                buf.append((board, cur_player, max_players, size))

                # 游戏结束 → 立即处理本局
                if game_over and winner is not None:
                    for b, cp, mp, sz in buf:
                        feats = extract_features(b, cp, mp, sz)
                        label = 1.0 if cp == winner else 0.0
                        features.append(feats)
                        labels.append(label)
                        valid += 1
                    games_found += 1
                    buf = []

        print(f'   ✓ {path.split("/")[-1]}: {valid} 条有效, {games_found} 局 / {total_lines} 行 (跳过 {skipped})')

    X = np.array(features, dtype=np.float32)
    y = np.array(labels, dtype=np.float32)
    return X, y


# ─── 模型训练 ───

def train_model(X, y):
    """训练 XGBoost 分类模型"""
    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.15, random_state=42, stratify=y
    )

    dtrain = xgb.DMatrix(X_train, label=y_train)
    dtest = xgb.DMatrix(X_test, label=y_test)

    params = {
        'objective': 'binary:logistic',
        'eval_metric': ['logloss', 'auc'],
        'max_depth': 5,
        'eta': 0.02,
        'subsample': 0.8,
        'colsample_bytree': 0.8,
        'min_child_weight': 3,
        'gamma': 0.1,
        'seed': 42,
        'verbosity': 0,
    }

    # 用 early stopping 避免过拟合
    model = xgb.train(
        params,
        dtrain,
        num_boost_round=800,
        evals=[(dtest, 'test')],
        early_stopping_rounds=30,
        verbose_eval=10,
    )

    # 评估
    y_pred = model.predict(dtest)
    y_pred_bin = (y_pred > 0.5).astype(int)
    acc = accuracy_score(y_test, y_pred_bin)
    auc = roc_auc_score(y_test, y_pred)

    print(f'\n📊 测试集结果: accuracy={acc:.4f}, AUC={auc:.4f}')
    print(f'🎯 最优树数: {model.best_iteration + 1}')

    return model


def export_to_xgb_json(model, output_path):
    """导出为 XGBoost JSON 格式（兼容 Rust 解析）"""
    model.save_model(output_path)
    with open(output_path) as f:
        raw = json.load(f)
    # 添加 features 字段（Rust 解析需要）
    raw.setdefault('feature_names', [str(i) for i in range(16)])
    raw.setdefault('feature_types', ['float'] * 16)
    with open(output_path, 'w') as f:
        json.dump(raw, f)
    print(f'✅ 模型已保存: {output_path}')


# ─── 主流程 ───

def main():
    if len(sys.argv) < 2:
        print('用法: python3 train_xgb_model.py <selfplay_data.jsonl> [more_files...] [--output <path>]')
        print('      默认输出到当前目录 xgb_model_board.json')
        sys.exit(1)

    args = sys.argv[1:]
    output_path = 'xgb_model_board.json'
    if '--output' in args:
        idx = args.index('--output')
        if idx + 1 < len(args):
            output_path = args[idx + 1]
            args = args[:idx] + args[idx + 2:]
        else:
            print('❌ --output 后需要路径参数')
            sys.exit(1)
    data_paths = args

    print('📥 加载数据...')
    X, y = load_selfplay_data(data_paths)
    print(f'   样本数: {len(X)}, 特征维度: {X.shape[1]}')
    print(f'   正样本(赢): {y.sum():.0f}, 负样本(输): {len(y) - y.sum():.0f}')

    if len(X) < 100:
        print('❌ 数据量太少，无法训练')
        sys.exit(1)

    print('\n🏋️  训练模型...')
    model = train_model(X, y)

    export_to_xgb_json(model, output_path)

    # 分析特征重要性
    importance = model.get_score(importance_type='gain')
    sorted_imp = sorted(importance.items(), key=lambda x: -x[1])
    print('\n📈 特征重要性 (gain):')
    for idx_str, gain in sorted_imp[:10]:
        print(f'   特征 {idx_str}: {gain:.2f}')


if __name__ == '__main__':
    main()
