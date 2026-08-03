#!/usr/bin/env python3
"""
连锁棋 ML 评估模型训练脚本（v2 — 多模式适配版）

数据格式（每行一条 JSON，由 selfplay 工具生成）:
  {"game_id":0,"turn":1,"size":7,"max_players":4,
   "border_mode":"default|wrap|bounce|degrade","cap_mode":"3|4|5|random",
   "board":[[{"owner":0,"count":1},...],...],
   "cur_player":0,"move_x":1,"move_y":2,"live_players":[...],
   "eliminated":[],"winner":null,"game_over":false}

特征（18 维 — 与 Rust 端 extract_features_improved 严格同步）:
  [0]  density               棋盘密度
  [1]  alive_ratio           存活比例
  [2]  my_share              我的棋子占比
  [3]  c3_share              三级棋子占比（语义保留，兼容旧模型）
  [4]  c2_share              二级棋子占比
  [5]  territory_share       地盘占比
  [6]  score_diff            分数差
  [7]  chain_threat_diff     爆发势能差（阈值感知）
  [8]  pos_bonus_diff        位置优势差
  [9]  threat_prox_diff      威胁逼近差（回环感知）
  [10] my_score_norm         我的分数归一
  [11] opp_score_norm        对手分数归一
  [12] center_dist_balance   中心距离差
  [13] territory_balance     地盘差
  [14] my_threat_ratio       我的三级威胁占比
  [15] opp_threat_ratio      对手三级威胁占比
  [16] my_crit_share         我的临界棋子占比（阈值感知，cap3→2/cap4→3/cap5→4）
  [17] opp_crit_share        对手临界棋子占比

用法:
  python3 train_xgb_model.py <selfplay_*.jsonl>... [--output <path>]
  python3 train_xgb_model.py --smoke              # 冒烟测试：合成数据跑通全流程
"""

import json
import sys
import numpy as np
import xgboost as xgb
from sklearn.model_selection import train_test_split
from sklearn.metrics import accuracy_score, roc_auc_score

try:
    from tqdm import tqdm
except ImportError:
    def tqdm(iterable, **kwargs):
        return iterable

FEAT_DIM = 18

# ─── 模式解析（与 Rust serde rename 一致） ───

def parse_border(s):
    return str(s or "default")

def parse_cap(s):
    return str(s or "4")

def cell_threshold(x, y, sz, border_mode, cap_mode):
    """格子的爆炸阈值（与 Rust cell_threshold 一致）"""
    if cap_mode == "3":
        return 3
    if cap_mode == "5":
        return 5
    if cap_mode == "random":
        return 4
    # cap_mode == "4"
    if border_mode == "degrade":
        on_corner = (x == 0 or x == sz - 1) and (y == 0 or y == sz - 1)
        on_edge = x == 0 or x == sz - 1 or y == 0 or y == sz - 1
        if on_corner:
            return 2
        if on_edge:
            return 3
    return 4

def crit_level(x, y, sz, border_mode, cap_mode):
    return cell_threshold(x, y, sz, border_mode, cap_mode) - 1

def nbrs_with_mode(i, j, sz, border_mode):
    """邻居（回环模式与 Rust nbrs_with_mode 一致）"""
    if border_mode == "wrap":
        up = sz - 1 if i == 0 else i - 1
        down = 0 if i + 1 >= sz else i + 1
        left = sz - 1 if j == 0 else j - 1
        right = 0 if j + 1 >= sz else j + 1
        return [(up, j), (down, j), (i, left), (i, right)]
    r = []
    if i > 0:
        r.append((i - 1, j))
    if i + 1 < sz:
        r.append((i + 1, j))
    if j > 0:
        r.append((i, j - 1))
    if j + 1 < sz:
        r.append((i, j + 1))
    return r

# ─── 特征提取（与 Rust extract_features_improved 严格同步） ───

def extract_features(board, cur_player, max_players, size, border_mode, cap_mode):
    total_cells = size * size
    total_pieces = sum(1 for row in board for c in row if c['owner'] is not None)

    my_score = 0; opp_score = 0
    my_territory = 0; opp_territory = 0
    my_chain_threat = 0; opp_chain_threat = 0
    my_pos_bonus = 0; opp_pos_bonus = 0
    my_threat_prox = 0; opp_threat_prox = 0
    my_pieces = 0; opp_pieces = 0
    my_c2 = 0; my_c3 = 0
    opp_c2 = 0; opp_c3 = 0
    my_crit = 0; opp_crit = 0
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
                    crit = crit_level(i, j, size, border_mode, cap_mode)

                    if p == cur_player:
                        my_score += cnt
                        my_territory += 1
                        my_pos_bonus += pos_val
                        my_cdist += abs(i - cx) + abs(j - cx)
                        my_pieces += 1
                        if cnt == 2:
                            my_c2 += 1
                        if cnt >= 3:
                            my_c3 += 1
                        if cnt >= crit:
                            my_chain_threat += cnt * 5
                            my_crit += 1
                        elif cnt >= crit - 1:
                            my_chain_threat += 2
                        for ni, nj in nbrs_with_mode(i, j, size, border_mode):
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
                        if cnt >= 3:
                            opp_c3 += 1
                        if cnt >= crit:
                            opp_chain_threat += cnt * 5
                            opp_crit += 1
                        elif cnt >= crit - 1:
                            opp_chain_threat += 2
                        for ni, nj in nbrs_with_mode(i, j, size, border_mode):
                            nc = board[ni][nj]
                            if nc['owner'] == cur_player:
                                opp_threat_prox += cnt
        if has_pieces:
            alive_count += 1

    total = max(total_pieces, 1)
    sz = float(size)

    feats = np.zeros(FEAT_DIM, dtype=np.float32)
    feats[0] = total_pieces / total_cells
    feats[1] = alive_count / max(max_players, 1)
    feats[2] = my_pieces / total
    feats[3] = my_c3 / max(my_pieces, 1)
    feats[4] = my_c2 / max(my_pieces, 1)
    feats[5] = my_territory / total_cells
    feats[6] = (my_score - opp_score) / total_cells
    feats[7] = (my_chain_threat - opp_chain_threat) / total_cells
    feats[8] = (my_pos_bonus - opp_pos_bonus) / total_cells
    feats[9] = (my_threat_prox - opp_threat_prox) / total_cells
    feats[10] = my_score / max(opp_score, 1)
    feats[11] = opp_score / max(my_score, 1)
    feats[12] = (my_cdist / max(my_pieces, 1) - opp_cdist / max(opp_pieces, 1)) / (sz * 0.5)
    feats[13] = (my_territory - opp_territory) / total_cells
    feats[14] = my_c3 / max(my_pieces, 1)
    feats[15] = opp_c3 / max(opp_pieces, 1)
    feats[16] = my_crit / max(my_pieces, 1)
    feats[17] = opp_crit / max(opp_pieces, 1)
    return feats


# ─── 数据加载 ───

def count_lines(path):
    with open(path, 'rb') as f:
        return sum(1 for _ in f)

def load_selfplay_data(data_paths):
    """从 selfplay JSONL 加载数据，按局用胜者标记每步标签。
    支持任意棋盘大小、玩家人数、边界模式、爆炸阈值模式（混合训练）。
    """
    features = []
    labels = []

    for path in data_paths:
        total_lines = count_lines(path)
        valid = 0
        skipped = 0
        games_found = 0
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
                if total_pieces == 0:
                    skipped += 1
                    continue

                if current_game is not None and game_id != current_game:
                    buf = []
                current_game = game_id

                game_over = rec.get('game_over', False)
                winner = rec.get('winner')
                cur_player = rec['cur_player']
                max_players = rec['max_players']
                size = rec['size']
                border_mode = parse_border(rec.get('border_mode'))
                cap_mode = parse_cap(rec.get('cap_mode'))

                buf.append((board, cur_player, max_players, size, border_mode, cap_mode))

                if game_over and winner is not None:
                    for b, cp, mp, sz_, bm, cm in buf:
                        feats = extract_features(b, cp, mp, sz_, bm, cm)
                        label = 1.0 if cp == winner else 0.0
                        features.append(feats)
                        labels.append(label)
                        valid += 1
                    games_found += 1
                    buf = []

        print(f'   ✓ {path.split("/")[-1]}: {valid} 条有效, {games_found} 局 / {total_lines} 行 (跳过 {skipped})')

    if not features:
        raise SystemExit('❌ 没有加载到任何有效样本')
    return np.array(features, dtype=np.float32), np.array(labels, dtype=np.float32)


# ─── 模型训练 ───

def train_model(X, y):
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

    model = xgb.train(
        params, dtrain,
        num_boost_round=800,
        evals=[(dtest, 'test')],
        early_stopping_rounds=30,
        verbose_eval=10,
    )

    y_pred = model.predict(dtest)
    y_pred_bin = (y_pred > 0.5).astype(int)
    acc = accuracy_score(y_test, y_pred_bin)
    auc = roc_auc_score(y_test, y_pred)
    print(f'\n📊 测试集结果: accuracy={acc:.4f}, AUC={auc:.4f}')
    print(f'🎯 最优树数: {model.best_iteration + 1}')
    return model


def export_to_xgb_json(model, output_path):
    """导出为 XGBoost JSON 格式（兼容 Rust 解析，18 维特征）"""
    model.save_model(output_path)
    with open(output_path) as f:
        raw = json.load(f)
    raw.setdefault('feature_names', [str(i) for i in range(FEAT_DIM)])
    raw.setdefault('feature_types', ['float'] * FEAT_DIM)
    with open(output_path, 'w') as f:
        json.dump(raw, f)
    print(f'✅ 模型已保存: {output_path}')


# ─── 冒烟测试：合成多模式数据跑通全流程 ───

def smoke_test():
    """用随机合成数据验证：特征提取 + 训练 + 导出，覆盖 cap3/4/5 × default/wrap"""
    import random
    rng = random.Random(42)
    rows = []
    combos = [
        (7, "default", "3"), (7, "default", "4"), (7, "default", "5"),
        (7, "wrap", "3"), (7, "wrap", "4"), (7, "wrap", "5"),
        (9, "degrade", "4"), (5, "bounce", "3"), (5, "bounce", "5"),
    ]
    game_id = 0
    for size, bm, cm in combos:
        for _ in range(4):  # 每组合 4 局
            for turn in range(8):
                board = [[{'owner': None, 'count': 0} for _ in range(size)] for _ in range(size)]
                for _ in range(rng.randint(3, size * size // 2)):
                    x, y = rng.randrange(size), rng.randrange(size)
                    board[x][y] = {'owner': rng.randrange(2), 'count': rng.randint(1, 4)}
                cur = rng.randrange(2)
                rows.append({
                    'game_id': game_id, 'turn': turn, 'size': size,
                    'max_players': 2, 'border_mode': bm, 'cap_mode': cm,
                    'board': board, 'cur_player': cur,
                    'winner': rng.randrange(2), 'game_over': turn == 7,
                })
            game_id += 1

    tmp = '/tmp/smoke_selfplay.jsonl'
    with open(tmp, 'w') as f:
        for r in rows:
            f.write(json.dumps(r) + '\n')

    X, y = load_selfplay_data([tmp])
    assert X.shape[1] == FEAT_DIM, f'特征维度必须为 {FEAT_DIM}，实际 {X.shape[1]}'
    assert len(X) >= 100, f'样本数不足: {len(X)}'
    print(f'   ✓ 特征维度 {FEAT_DIM}，样本 {len(X)}')

    # 验证特征有区分度（临界占比特征非全零）
    crit_cols = X[:, 16:18]
    assert crit_cols.max() > 0, '临界占比特征全为 0，特征提取有问题'

    model = train_model(X, y)
    out = '/tmp/xgb_smoke.json'
    export_to_xgb_json(model, out)

    # 验证导出 JSON 可被 Rust 结构解析的关键字段
    with open(out) as f:
        raw = json.load(f)
    learner = raw['learner']['gradient_booster']['model']['trees']
    assert len(learner) > 0, '模型树为空'
    print(f'   ✓ 模型导出成功: {len(learner)} 棵树')
    print('✅ 冒烟测试通过')


# ─── 主流程 ───

def main():
    if '--smoke' in sys.argv:
        print('🧪 冒烟测试（合成数据）...')
        smoke_test()
        return

    if len(sys.argv) < 2:
        print('用法: python3 train_xgb_model.py <selfplay_data.jsonl> [more_files...] [--output <path>]')
        print('      或: python3 train_xgb_model.py --smoke')
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
    if X.shape[1] != FEAT_DIM:
        print(f'⚠️  特征维度 {X.shape[1]} != 期望 {FEAT_DIM}，检查 selfplay 数据是否包含 border_mode/cap_mode')
    print(f'   正样本(赢): {y.sum():.0f}, 负样本(输): {len(y) - y.sum():.0f}')

    if len(X) < 100:
        print('❌ 数据量太少，无法训练（至少 100 条）')
        sys.exit(1)

    print('\n🏋️  训练模型...')
    model = train_model(X, y)
    export_to_xgb_json(model, output_path)

    importance = model.get_score(importance_type='gain')
    sorted_imp = sorted(importance.items(), key=lambda x: -x[1])
    print('\n📈 特征重要性 (gain):')
    for idx_str, gain in sorted_imp[:10]:
        print(f'   特征 {idx_str}: {gain:.2f}')


if __name__ == '__main__':
    main()
