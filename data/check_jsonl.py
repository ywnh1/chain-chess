#!/usr/bin/env python3
import json

path = '/home/ywnh1/Programs/chain-chess/tauri/src-tauri/data/selfplay_9x4_500.jsonl'
games = set()
completed = set()
incomplete_last_turn = {}
total_steps = 0

with open(path) as f:
    for line in f:
        d = json.loads(line)
        gid = d['game_id']
        games.add(gid)
        total_steps += 1
        if d['game_over']:
            completed.add(gid)
        else:
            incomplete_last_turn[gid] = d['turn']

print(f"总行数(步数): {total_steps}")
print(f"总游戏数: {len(games)}")
print(f"已完成游戏: {len(completed)}")
print(f"游戏ID范围: {min(games)} - {max(games)}")
print(f"未完成游戏数: {len(games) - len(completed)}")
if incomplete_last_turn:
    last_id = max(incomplete_last_turn.keys())
    print(f"最后一个未完成游戏的ID: {last_id}, 回合: {incomplete_last_turn[last_id]}")
