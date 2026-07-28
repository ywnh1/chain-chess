#!/usr/bin/env python3
"""检查 XGBoost 模型结构"""
import json

with open('/home/ywnh1/Programs/chain-chess/xgb_model_fast.json') as f:
    m = json.load(f)

gb = m['learner']['gradient_booster']['model']
trees = gb['trees']
print(f"树数量: {len(trees)}")
print(f"特征数: {trees[0]['tree_param']['num_feature']}")
print(f"基础分: {m['learner']['learner_model_param']['base_score']}")
print(f"目标函数: {m['learner']['objective']['name']}")

for ti in range(min(3, len(trees))):
    t = trees[ti]
    print(f"\n树{ti}: 字段={list(t.keys())}")
    for k, v in t.items():
        if isinstance(v, list):
            if len(v) > 0:
                print(f"  {k}: list[{len(v)}] first={v[0]} last={v[-1]}")
            else:
                print(f"  {k}: list[{len(v)}] (empty)")
        elif isinstance(v, dict):
            print(f"  {k}: dict {dict(v)}")
        else:
            print(f"  {k}: {v}")
    # 展示前几个分支节点
    print(f"  --- 前3个分支 ---")
    for ni in range(min(3, len(t['id']))):
        is_leaf = t['left_children'][ni] == -1 and t['right_children'][ni] == -1
        if is_leaf:
            print(f"  [{ni}] id={t['id'][ni]} LEAF val={t['base_weights'][ni]}")
        else:
            print(f"  [{ni}] id={t['id'][ni]} feat={t['split_indices'][ni]} cond={t['split_conditions'][ni]} left={t['left_children'][ni]} right={t['right_children'][ni]}")
