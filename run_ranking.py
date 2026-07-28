#!/usr/bin/env python3
"""AI 战力排位赛——循环赛 10 局制，生成 markdown 报告"""

import json
import subprocess
import sys
import os
import tempfile

PROJECT_ROOT = os.path.dirname(os.path.abspath(__file__))
BATTLE_BIN = os.path.join(PROJECT_ROOT, "tauri", "src-tauri", "target", "release", "battle")

# ─── 参赛选手（depth=2 标准配置） ───
COMPETITORS = [
    {"id": "strategy",   "alg": "strategy",  "depth": None, "ml": False, "label": "策略算法"},
    {"id": "ab_hand",    "alg": "alphabeta", "depth": 2,    "ml": False, "label": "Alpha-Beta 手写"},
    {"id": "ab_ml",      "alg": "alphabeta", "depth": 2,    "ml": True,  "label": "Alpha-Beta ML"},
    {"id": "ab_hand_d1", "alg": "alphabeta", "depth": 1,    "ml": False, "label": "Alpha-Beta 手写 d1"},
    {"id": "ab_ml_d1",   "alg": "alphabeta", "depth": 1,    "ml": True,  "label": "Alpha-Beta ML d1"},
    {"id": "mcts",       "alg": "mcts",      "depth": 2,    "ml": False, "label": "MCTS d2"},
]

GAMES_PER_MATCHUP = 10
BOARD_SIZE = 7

def make_config(a, b):
    """为两个选手生成 battle 配置"""
    def entry(c):
        e = {"type": c["alg"]}
        if c["depth"] is not None:
            e["depth"] = c["depth"]
        if c["ml"]:
            e["use_ml_eval"] = True
        else:
            e["use_ml_eval"] = False
        e["name"] = c["label"]
        return e

    return {
        "size": BOARD_SIZE,
        "times": GAMES_PER_MATCHUP,
        "ai": [entry(a), entry(b)],
    }


def run_battle(config_path):
    """运行一次 battle 并返回 (wins_a, wins_b, draws, total, time_s)"""
    env = os.environ.copy()
    env["RUST_BACKTRACE"] = "0"
    try:
        r = subprocess.run(
            [BATTLE_BIN, config_path],
            capture_output=True, text=True, timeout=600, env=env
        )
    except subprocess.TimeoutExpired:
        return None
    except FileNotFoundError:
        return None

    # 解析最后一行 "RESULT: ..."
    for line in reversed(r.stdout.splitlines()):
        line = line.strip()
        if line.startswith("RESULT:"):
            parts = line.replace("RESULT: ", "").split()
            # format: P0=x P1=y DRAW=z TOTAL=N TIME=Ts
            p0_wins = 0
            p1_wins = 0
            draws = 0
            total = 0
            time_s = 0
            for p in parts:
                if "=" in p:
                    k, v = p.split("=", 1)
                    if k == "DRAW":
                        draws = int(v)
                    elif k == "TOTAL":
                        total = int(v)
                    elif k == "TIME":
                        time_s = float(v.rstrip("s"))
                    elif k == "P0":
                        p0_wins = int(v)
                    elif k == "P1":
                        p1_wins = int(v)
            return [p0_wins, p1_wins], draws, total, time_s
    return None


def main():
    if not os.path.exists(BATTLE_BIN):
        # 尝试编译
        print("🔨 编译 battle binary...")
        r = subprocess.run(
            ["cargo", "build", "--release", "--bin", "battle"],
            cwd=os.path.join(PROJECT_ROOT, "tauri", "src-tauri"),
            capture_output=True, text=True
        )
        if r.returncode != 0:
            print("❌ 编译失败:", r.stderr)
            sys.exit(1)
        print("✅ 编译完成")

    if not os.path.exists(BATTLE_BIN):
        print(f"❌ {BATTLE_BIN} 不存在")
        sys.exit(1)

    # ─── 生成所有配对 ───
    n = len(COMPETITORS)
    matchups = []
    for i in range(n):
        for j in range(i + 1, n):
            matchups.append((i, j))

    print(f"🏟  参赛选手: {n} 个")
    print(f"   对战组合: {len(matchups)} 组")
    print(f"   每局: {GAMES_PER_MATCHUP} 场")
    print(f"   总计: {len(matchups) * GAMES_PER_MATCHUP} 场")
    print()

    # ─── 对战记录 ───
    # wins[i][j] = 选手 i 对选手 j 的胜场
    wins = [[0] * n for _ in range(n)]

    tmpdir = tempfile.mkdtemp()
    try:
        for idx, (i, j) in enumerate(matchups):
            a = COMPETITORS[i]
            b = COMPETITORS[j]
            config_path = os.path.join(tmpdir, f"match_{a['id']}_vs_{b['id']}.json")

            with open(config_path, "w") as f:
                json.dump(make_config(a, b), f, ensure_ascii=False, indent=2)

            print(f"[{idx+1}/{len(matchups)}] {a['label']} vs {b['label']} ...", end=" ", flush=True)
            result = run_battle(config_path)

            if result is None:
                print("❌ 失败")
                continue

            scores, draws, total, ts = result
            win_a, win_b = scores[0], scores[1]
            wins[i][j] = win_a
            wins[j][i] = win_b
            print(f"{a['label']} {win_a} - {b['label']} {win_b} (平{draws}) [{ts:.1f}s]")
    finally:
        import shutil
        shutil.rmtree(tmpdir)

    # ─── 计算积分 ───
    # 赢=3分 平=1分 输=0分
    points = [0] * n
    for i in range(n):
        for j in range(n):
            if i == j:
                continue
            wi = wins[i][j]
            wj = wins[j][i]
            draws = GAMES_PER_MATCHUP - wi - wj
            points[i] += wi * 3 + draws * 1

    # 排名
    ranked = sorted(range(n), key=lambda i: (-points[i], -sum(wins[i])))

    # ─── 生成 markdown ───
    lines = []
    lines.append("# ♟ 连锁棋 AI 战力排行榜")
    lines.append("")
    lines.append(f"> 测试条件: 7×7 棋盘，深度 2，每组对战 {GAMES_PER_MATCHUP} 局，先手交替")
    lines.append("")
    lines.append("## 最终排名")
    lines.append("")
    lines.append("| 排名 | 选手 | 积分 | 胜率 | 描述 |")
    lines.append("|------|------|------|------|------|")

    for rank, idx in enumerate(ranked, 1):
        c = COMPETITORS[idx]
        total_games = (n - 1) * GAMES_PER_MATCHUP
        w = sum(wins[idx])
        total_wins = w
        total_games_played = total_games
        pct = total_wins / total_games_played * 100 if total_games_played > 0 else 0
        lines.append(f"| {rank} | {c['label']} | {points[idx]} | {total_wins}/{total_games_played} ({pct:.0f}%) | {c['alg']} depth={c['depth'] or '—'} ML={'✓' if c['ml'] else '—'} |")

    lines.append("")
    lines.append("## 对战详情")
    lines.append("")
    lines.append("| 胜方 | 负方 | 比分 |")
    lines.append("|------|------|------|")

    for i in range(n):
        for j in range(n):
            if i >= j:
                continue
            wi = wins[i][j]
            wj = wins[j][i]
            draws = GAMES_PER_MATCHUP - wi - wj
            if wi > wj:
                lines.append(f"| {COMPETITORS[i]['label']} | {COMPETITORS[j]['label']} | {wi}–{wj}" + (f" (平{draws})" if draws else "") + " |")
            elif wj > wi:
                lines.append(f"| {COMPETITORS[j]['label']} | {COMPETITORS[i]['label']} | {wj}–{wi}" + (f" (平{draws})" if draws else "") + " |")
            else:
                lines.append(f"| {COMPETITORS[i]['label']} | {COMPETITORS[j]['label']} | {wi}–{wj} (平{draws}) |")

    lines.append("")
    lines.append("## 分析")
    lines.append("")

    # 简单分析
    top = COMPETITORS[ranked[0]]
    top_w = sum(wins[ranked[0]])
    top_pct = top_w / ((n - 1) * GAMES_PER_MATCHUP) * 100
    lines.append(f"- **{top['label']}** 以 {top_pct:.0f}% 胜率排名第一，是当前最强 AI。")

    # ML vs 手写
    ml_wins = sum(sum(wins[i] for i, c in enumerate(COMPETITORS) if c["ml"])
    hand_wins = sum(sum(wins[i] for i, c in enumerate(COMPETITORS) if not c["ml"] and c["alg"] != "strategy")
    lines.append(f"- ML 评估（含 alphabeta/pvs × ML）总胜场 {ml_wins}，手写评估总胜场 {hand_wins}。")
    if ml_wins > hand_wins:
        lines.append("- ML 评估显著优于手写评估。")
    else:
        lines.append("- 手写评估仍具竞争力。")

    # 算法比较
    ab_wins = sum(sum(wins[i] for i, c in enumerate(COMPETITORS) if c["alg"] == "alphabeta")
    d1_wins = sum(sum(wins[i] for i, c in enumerate(COMPETITORS) if c["alg"] == "alphabeta" and c["depth"] == 1)
    d2_wins = sum(sum(wins[i] for i, c in enumerate(COMPETITORS) if c["alg"] == "alphabeta" and c["depth"] == 2)
    mcts_wins = sum(wins[i] for i, c in enumerate(COMPETITORS) if c["alg"] == "mcts")
    lines.append(f"- Alpha-Beta 总胜场 {ab_wins}（d1: {d1_wins}，d2: {d2_wins}），MCTS 总胜场 {mcts_wins}，策略算法总胜场 {strat_wins}。")

    lines.append("")
    lines.append("> 生成时间: " + __import__("datetime").datetime.now().strftime("%Y-%m-%d %H:%M"))

    report = "\n".join(lines)

    report_path = os.path.join(PROJECT_ROOT, "AI_RANKING.md")
    with open(report_path, "w") as f:
        f.write(report)
    print(f"\n✅ 报告已生成: {report_path}")
    print(report)


if __name__ == "__main__":
    main()
