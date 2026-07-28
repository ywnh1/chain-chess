#!/usr/bin/env python3
"""快速调试训练脚本"""
import sys, json
sys.path.insert(0, "/home/ywnh1/Programs/chain-chess")
from data.train_xgb import load_jsonl, prepare_dataset, train

path = "/tmp/test_subset.jsonl"
print(f"加载: {path}")
games = load_jsonl(path)
print(f"  游戏: {len(games)}")

print("提取特征...")
X, y = prepare_dataset(games, max_players=4, size=9)
print(f"  X: {X.shape}, y: {y.shape}")
print(f"  正样本率: {y.mean():.3f}")

print("训练...")
model = train(X, y)
print("✅ 成功!")
