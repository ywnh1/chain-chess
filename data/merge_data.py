#!/usr/bin/env python3
"""
合并多份 JSONL 数据为一份，重新分配 game_id 避免冲突。
跳过未完成的游戏。输出各文件的数据配置摘要。

用法:
  python3 data/merge_data.py [--output ./data/selfplay_merged.jsonl] [--input-dir <目录>]
"""

import json, os, sys, argparse
from collections import defaultdict

def main():
    parser = argparse.ArgumentParser(description="合并 JSONL 数据")
    parser.add_argument("--input-dir", default=None)
    parser.add_argument("--output", default="./data/selfplay_merged.jsonl")
    args = parser.parse_args()

    # 确定输入目录
    script_dir = os.path.dirname(os.path.abspath(__file__))
    if args.input_dir:
        in_dir = os.path.normpath(args.input_dir)
    else:
        # 从当前工作目录开始找
        cwd = os.getcwd()
        candidates = [
            os.path.join(cwd, "tauri", "src-tauri", "data"),
            os.path.join(cwd, "data"),
            os.path.join(script_dir),
            os.path.join(script_dir, "..", "tauri", "src-tauri", "data"),
        ]
        in_dir = None
        for c in candidates:
            if os.path.isdir(c):
                jsonls = [f for f in os.listdir(c)
                          if f.endswith(".jsonl") and os.path.getsize(os.path.join(c, f)) > 1000]
                if jsonls:
                    in_dir = c
                    break
        if not in_dir:
            print("❌ 未找到包含 .jsonl 的目录")
            print("   请指定: --input-dir <目录>")
            sys.exit(1)

    files = sorted([f for f in os.listdir(in_dir)
                    if f.endswith(".jsonl") and os.path.getsize(os.path.join(in_dir, f)) > 1000])
    if not files:
        print(f"❌ {in_dir} 下没有有效的 .jsonl 文件")
        sys.exit(1)

    print(f"📂 输入目录: {in_dir}")
    print(f"📄 文件: {len(files)} 个\n")

    output_path = os.path.normpath(args.output)
    os.makedirs(os.path.dirname(output_path) or ".", exist_ok=True)

    new_gid = 0
    total_games = 0
    total_steps = 0
    skipped_games = 0

    with open(output_path, "w", encoding="utf-8") as out:
        for fname in files:
            path = os.path.join(in_dir, fname)
            mb = os.path.getsize(path) / 1024 / 1024
            print(f"  📖 {fname} ({mb:.0f}MB)...", end=" ", flush=True)

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
                            print(f"\n  ⚠️  第{line_no}行 JSON 解析失败: {e}")

            if bad_lines:
                print(f"  ⚠️  共跳过 {bad_lines} 行损坏数据")

            file_games = 0
            file_skipped = 0
            configs = set()

            for gid in sorted(games.keys()):
                steps = games[gid]
                if not steps[-1].get("game_over", False):
                    file_skipped += 1
                    continue
                configs.add((steps[0].get("size", "?"), steps[0].get("max_players", "?")))
                for s in steps:
                    s["game_id"] = new_gid
                    out.write(json.dumps(s, ensure_ascii=False) + "\n")
                    total_steps += 1
                new_gid += 1
                file_games += 1

            total_games += file_games
            skipped_games += file_skipped

            config_str = ", ".join(f"{s}×{s}/{p}p" for s, p in sorted(configs))
            print(f"{file_games}局", end="")
            if file_skipped:
                print(f" (跳过{file_skipped})", end="")
            print(f"  [{config_str}]")

    mb_out = os.path.getsize(output_path) / 1024 / 1024
    print(f"\n✅ 合并完成!")
    print(f"   输出: {output_path} ({mb_out:.0f}MB)")
    print(f"   游戏: {total_games} 局 (跳过 {skipped_games})")
    print(f"   步数: {total_steps} 步")


if __name__ == "__main__":
    main()
