#!/usr/bin/env python3
"""检查 JSONL 文件统计信息"""
import json, os

data_dir = "/home/ywnh1/Programs/chain-chess/tauri/src-tauri/data"
files = sorted(f for f in os.listdir(data_dir) if f.endswith(".jsonl"))

for fname in files:
    path = os.path.join(data_dir, fname)
    mb = os.path.getsize(path) / 1024 / 1024
    gids = set()
    steps = 0
    completed = 0

    with open(path) as f:
        for line in f:
            d = json.loads(line)
            steps += 1
            gids.add(d["game_id"])
            if d["game_over"]:
                completed += 1

    print(f"{fname:30s} {steps:>8}步  {mb:>6.0f}MB  {len(gids):>4}局(完成{completed})  game_id={min(gids)}-{max(gids)}")
