#!/usr/bin/env python3
"""检查合并后的数据集配置分布"""
import json, sys
from collections import defaultdict, Counter

path = "./data/selfplay_merged.jsonl" if len(sys.argv) < 2 else sys.argv[1]

games = defaultdict(list)
bad = 0
with open(path, encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if not line: continue
        try:
            d = json.loads(line)
            games[d["game_id"]].append(d)
        except json.JSONDecodeError:
            bad += 1

print(f"总步数: {sum(len(v) for v in games.values())}")
print(f"总游戏: {len(games)}")
if bad:
    print(f"损坏行: {bad}")

cfgs = Counter()
for gid, steps in games.items():
    s = steps[0]
    cfgs[(s.get("size", "?"), s.get("max_players", "?"), len(steps))] += 1

print(f"\n配置分布:")
for (size, players, avg_steps), count in sorted(cfgs.items()):
    print(f"  {size}×{size} / {players}人  ~{avg_steps}步/局  = {count}局")
