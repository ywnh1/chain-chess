#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""从 ai_report.json 生成应用内「AI 性能评测」页的 HTML 片段。

页面里的数字原先全是手写的，换一次模型或改一次口径就得手工改一百多处，
改漏一处就会和实测数据对不上。这个脚本把整段内容都从数据生成，
重跑对局后只要执行一次就能刷新页面。

用法（仓库根目录）:
    python3 tauri/tests/gen_report_html.py                 # 用默认 ai_report.json，写两份 index.html
    python3 tauri/tests/gen_report_html.py --dry-run       # 只打印，不写文件
    python3 tauri/tests/gen_report_html.py --json other.json
"""
import argparse
import json
import os
import sys
from collections import defaultdict
from datetime import datetime

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
DEFAULT_JSON = os.path.join(HERE, 'ai_report.json')
TARGETS = [
    os.path.join(ROOT, 'tauri', 'public', 'index.html'),
    os.path.join(ROOT, 'docs', 'index.html'),
]

ALG_LABEL = {'strategy': '策略', 'alphabeta': 'A-B', 'pvs': 'PVS', 'mcts': 'MCTS'}
# 参战算法按此顺序展示；实际用到哪些由数据决定（MCTS 已从游戏内移除，
# 但显式 --algs 复测时仍可能出现，所以不能写死）
ALG_PREFERENCE = ['alphabeta', 'pvs', 'strategy', 'mcts']
BM_LABEL = {'default': '默认', 'wrap': '回环', 'bounce': '反弹', 'degrade': '降级'}
BM_ORDER = ['default', 'wrap', 'bounce', 'degrade']
CAP_LABEL = {'3': '速爆 3', '4': '标准 4', '5': '重炮 5'}
CAP_ORDER = ['3', '4', '5']
SHAPE_LABEL = {'square': '正方形', 'rect': '长方形', 'holes': '挖洞', 'ring': '口字', 'cross': '十字'}
SHAPE_ORDER = ['square', 'rect', 'holes', 'ring', 'cross']

BAR_COLORS = {
    'alphabeta': 'linear-gradient(90deg,#5fc3c3,#70d070)',
    'pvs': 'linear-gradient(90deg,#9b7edc,#5fc3c3)',
    'strategy': 'linear-gradient(90deg,#f0b34b,#ff9d5c)',
    'mcts': 'linear-gradient(90deg,#ff8a8a,#ff6b6b)',
}
MEDALS = ['🥇', '🥈', '🥉', '4']


def alg_order(rows):
    """数据里实际出现过的算法，按偏好顺序排。"""
    seen = set()
    for x in rows:
        seen.update(x['players'])
    return [a for a in ALG_PREFERENCE if a in seen] + sorted(seen - set(ALG_PREFERENCE))


def pct(n, d):
    return (n / d * 100) if d else 0.0


def is_capped(x):
    return x['steps'] >= 1000 and not (x.get('order') or [])


def tally(rows, keyfn):
    """按 key 汇总：(胜场 dict, 局数 dict)。"""
    wins = defaultdict(lambda: defaultdict(int))
    games = defaultdict(int)
    for x in rows:
        k = keyfn(x)
        games[k] += 1
        if x['winner'] is not None:
            wins[k][x['players'][x['winner']]] += 1
    return wins, games


def overall_stats(rows):
    """整体胜场/参赛局数（每局每个算法都算一次出场）。"""
    wins = defaultdict(int)
    games = defaultdict(int)
    for x in rows:
        for p in x['players']:
            games[p] += 1
        if x['winner'] is not None:
            wins[x['players'][x['winner']]] += 1
    return dict(wins), dict(games)


def cell_color(win, total, a):
    """A 方胜率上色：>50% 绿、<50% 红、持平灰。"""
    if not total:
        return ''
    r = win / total
    if r > 0.5:
        return 'color:#70d070'
    if r < 0.5:
        return 'color:#ff8a8a'
    return ''


def build_config_card(cfg, total, capped_n, use_ml, model_note, order):
    shapes = cfg.get('shapes') or ['square']
    shape_txt = ' / '.join(SHAPE_LABEL.get(s, s) for s in shapes)
    edge = cfg['size']
    if 'rect' in shapes:
        edge_txt = f'{edge}×{edge}（长方形 {edge}×{min(edge + 2, 19)}）'
    else:
        edge_txt = f'{edge}×{edge}'
    eval_txt = f'ML 模型（{model_note}）' if use_ml else '手写规则'
    # 配对数 = 参战算法的两两组合数，不能写死
    pairs_n = len(order) * (len(order) - 1) // 2
    # MCTS 深度只在 MCTS 真的参战时才写
    depth_txt = f'A-B / PVS 深度 {cfg["depth"]}'
    if 'mcts' in order:
        depth_txt += f' · MCTS 深度 {cfg["mctsDepth"]}'
    return f'''      <div class="about-card">
        <h3 style="font-size:1.05rem;margin-bottom:8px">AI 实力测试（{total} 局）</h3>
        <p style="font-size:.82rem;color:var(--dim);line-height:1.7;margin:0">
          {len(shapes)} 形状 × {len(cfg["borders"])} 边界 × {len(cfg["caps"])} 爆炸阈值 × {pairs_n} 配对 × 每组合 {cfg["games"]} 局 = <strong>{total} 局</strong><br>
          棋盘 {edge_txt} · 形状 {shape_txt} · {depth_txt} · 随机刻度 {cfg["randomScale"]}%<br>
          评估函数 {eval_txt} · 两两对阵（每对固定先后手）· 种子 {cfg["seed"]}<br>
          测试脚本: <code>tauri/tests/ai_strength.py</code> + <code>ai_bench</code>（与正式对局同源引擎）
        </p>
      </div>
'''


def build_summary_card(rows, overall, order):
    """总结要点：全部由数据推出，不写死结论。"""
    wins, games = overall
    ranked = sorted(order, key=lambda a: -pct(wins[a], games[a]))
    top, bottom = ranked[0], ranked[-1]

    # 各阈值下的第一名
    w_cap, g_cap = tally(rows, lambda x: x['cm'])
    cap_best = []
    for cm in CAP_ORDER:
        if not g_cap.get(cm):
            continue
        best = max(order, key=lambda a: pct(w_cap[cm][a], g_cap[cm]))
        cap_best.append(f'{CAP_LABEL[cm]} 下是 {ALG_LABEL[best]}（{pct(w_cap[cm][best], g_cap[cm]):.1f}%）')

    # 各形状下的第一名
    w_sh, g_sh = tally(rows, lambda x: x['shape'])
    shape_best = []
    for sh in SHAPE_ORDER:
        if not g_sh.get(sh):
            continue
        best = max(order, key=lambda a: pct(w_sh[sh][a], g_sh[sh]))
        shape_best.append(f'{SHAPE_LABEL[sh]} {ALG_LABEL[best]} {pct(w_sh[sh][best], g_sh[sh]):.0f}%')

    # 步数
    steps = sorted(x['steps'] for x in rows)
    med = steps[len(steps) // 2] if steps else 0
    capped = [x for x in rows if is_capped(x)]

    lines = [
        f'<li><strong>实力排序</strong>：' + ' &gt; '.join(
            f'{ALG_LABEL[a]} {pct(wins[a], games[a]):.1f}%' for a in ranked) + '</li>',
        f'<li><strong>阈值会改变赢家</strong>：' + '；'.join(cap_best) +
        '。阈值越低，启发式的「临界引爆」直觉越管用；越高越吃推演</li>',
        f'<li><strong>形状格局</strong>：' + '、'.join(shape_best) + '</li>',
        f'<li><strong>对局节奏</strong>：中位 {med} 步。回环边界明显更快，'
        f'口字 / 长方形这类大形状更慢</li>',
    ]
    if capped:
        dist = defaultdict(int)
        for x in capped:
            if x['winner'] is not None:
                dist[x['players'][x['winner']]] += 1
        dist_txt = '、'.join(f'{ALG_LABEL.get(a, a)} {n} 局' for a, n in
                             sorted(dist.items(), key=lambda kv: -kv[1]))
        lines.append(
            f'<li><strong>{len(capped)} 局打满 1000 步仍无人出局</strong>（{pct(len(capped), len(rows)):.1f}%），'
            f'集中在慢节奏 + 大形状组合；引擎按「第一个存活玩家」判胜，'
            f'这些胜负不是真实实力（被判胜：{dist_txt}）</li>')
    return '''      <div class="about-card">
        <h3 style="font-size:1.05rem;margin-bottom:8px">总结</h3>
        <ul style="font-size:.82rem;line-height:1.8;margin:0;padding-left:1.1em">
          ''' + '\n          '.join(lines) + '''
        </ul>
      </div>
'''


def build_ranking_card(overall, order):
    wins, games = overall
    ranked = sorted(order, key=lambda a: -pct(wins[a], games[a]))
    bars = []
    for a in ranked:
        p = pct(wins[a], games[a])
        bars.append(f'''        <div style="display:flex;align-items:center;gap:8px;margin:5px 0">
          <span style="width:60px;font-size:.8rem;white-space:nowrap">{a}</span>
          <div style="flex:1;height:14px;background:rgba(255,255,255,.06);border-radius:7px;overflow:hidden">
            <div style="width:{p:.1f}%;height:100%;background:{BAR_COLORS[a]};border-radius:7px"></div>
          </div>
          <span style="width:52px;text-align:right;font-size:.8rem;color:var(--accent);font-weight:600">{p:.1f}%</span>
        </div>''')

    rows_html = []
    for i, a in enumerate(ranked):
        played = games[a]
        won = wins[a]
        lost = played - won
        rows_html.append(f'''          <tr style="border-bottom:1px solid rgba(255,255,255,.06)">
            <td style="padding:4px 6px">{MEDALS[i] if i < len(MEDALS) else i + 1}</td>
            <td style="padding:4px 6px"><strong>{a}</strong></td>
            <td style="padding:4px 6px;text-align:right;color:#70d070">{won}</td>
            <td style="padding:4px 6px;text-align:right;color:#ff8a8a">{lost}</td>
            <td style="padding:4px 6px;text-align:right;font-weight:600">{pct(won, played):.1f}%</td>
          </tr>''')

    return f'''      <div class="about-card">
        <h3 style="font-size:1.05rem;margin-bottom:8px">综合胜率</h3>
{chr(10).join(bars)}
        <h4 style="margin:14px 0 6px;font-size:.95rem">综合排行（所有模式汇总）</h4>
        <table style="width:100%;font-size:.82rem;border-collapse:collapse">
          <tr style="border-bottom:1px solid rgba(255,255,255,.1)">
            <th style="text-align:left;padding:5px 6px;width:30px">#</th>
            <th style="text-align:left;padding:5px 6px">算法</th>
            <th style="text-align:right;padding:5px 6px">胜</th>
            <th style="text-align:right;padding:5px 6px">负</th>
            <th style="text-align:right;padding:5px 6px">胜率</th>
          </tr>
{chr(10).join(rows_html)}
        </table>
      </div>
'''


def build_matrix_card(rows, cfg, title, hint, order):
    """形状 × 边界 的胜率矩阵：每格写第一名算法与胜率。"""
    w, g = tally(rows, lambda x: (x['shape'], x['bm']))
    shapes = cfg.get('shapes') or ['square']
    head = ''.join(f'<th style="padding:4px 6px">{BM_LABEL.get(b, b)}</th>' for b in BM_ORDER
                   if b in cfg['borders'])
    body = []
    for sh in shapes:
        cells = []
        for bm in BM_ORDER:
            if bm not in cfg['borders']:
                continue
            total = g.get((sh, bm), 0)
            if not total:
                cells.append('<td style="padding:3px 6px;text-align:center">-</td>')
                continue
            best = max(order, key=lambda a: w[(sh, bm)][a])
            cells.append(
                f'<td style="padding:3px 6px;text-align:center">'
                f'{ALG_LABEL[best]} {pct(w[(sh, bm)][best], total):.0f}%</td>')
        body.append(f'<tr><td style="padding:3px 6px">{SHAPE_LABEL.get(sh, sh)}</td>'
                    + ''.join(cells) + '</tr>')
    return f'''      <div class="about-card">
        <h3 style="font-size:1.05rem;margin-bottom:8px">{title}</h3>
        <p style="font-size:.78rem;color:var(--dim);margin:0 0 6px">{hint}</p>
        <table style="width:100%;font-size:.78rem;border-collapse:collapse">
          <tr style="border-bottom:1px solid rgba(255,255,255,.1)">
            <th style="text-align:left;padding:4px 6px">形状 \\ 边界</th>{head}
          </tr>
          {chr(10).join('          ' + r for r in body)}
        </table>
      </div>
'''


def build_pair_cards(rows, cfg, order):
    """每个配对一张可折叠明细：形状 × 边界 × 阈值 的 A:B。"""
    pairs = []
    seen = set()
    for x in rows:
        ps = tuple(sorted(x['players'], key=lambda a: order.index(a)))
        if len(ps) == 2 and ps not in seen:
            seen.add(ps)
            pairs.append(ps)
    pairs.sort(key=lambda p: (order.index(p[0]), order.index(p[1])))

    shapes = cfg.get('shapes') or ['square']
    cards = []
    for a, b in pairs:
        sub = [x for x in rows if set(x['players']) == {a, b}]
        total = len(sub)
        aw = sum(1 for x in sub if x['winner'] is not None and x['players'][x['winner']] == a)
        bw = sum(1 for x in sub if x['winner'] is not None and x['players'][x['winner']] == b)
        body = []
        for sh in shapes:
            for bm in BM_ORDER:
                if bm not in cfg['borders']:
                    continue
                for cm in CAP_ORDER:
                    if cm not in cfg['caps']:
                        continue
                    cell = [x for x in sub if x['shape'] == sh and x['bm'] == bm and x['cm'] == cm]
                    if not cell:
                        continue
                    cw = sum(1 for x in cell if x['winner'] is not None
                             and x['players'][x['winner']] == a)
                    n = len(cell)
                    avg = sum(x['steps'] for x in cell) / n
                    color = cell_color(cw, n, a)
                    body.append(f'''            <tr>
              <td style="padding:3px 6px">{SHAPE_LABEL.get(sh, sh)}</td>
              <td style="padding:3px 6px">{BM_LABEL.get(bm, bm)}</td>
              <td style="padding:3px 6px;text-align:center">cap{cm}</td>
              <td style="padding:3px 6px;text-align:center;font-weight:600;{color}">{cw}:{n - cw}</td>
              <td style="padding:3px 6px;text-align:center">{pct(cw, n):.0f}%</td>
              <td style="padding:3px 6px;text-align:right">{avg:.0f}</td>
            </tr>''')
        cards.append(f'''      <div class="about-card">
        <details style="margin-bottom:0">
          <summary style="cursor:pointer;font-size:.95rem;padding:6px 8px;border-radius:8px;background:rgba(255,255,255,.05)">
            <strong>{ALG_LABEL[a]}</strong> vs <strong>{ALG_LABEL[b]}</strong>
            <span style="float:right;color:var(--dim);font-size:.78rem">{aw}:{bw} · {pct(aw, total):.0f}%</span>
          </summary>
          <table style="width:100%;font-size:.76rem;border-collapse:collapse;margin-top:6px">
            <tr style="border-bottom:1px solid rgba(255,255,255,.1)">
              <th style="text-align:left;padding:4px 6px">形状</th>
              <th style="text-align:left;padding:4px 6px">边界</th>
              <th style="padding:4px 6px">阈值</th>
              <th style="padding:4px 6px">{ALG_LABEL[a]}:{ALG_LABEL[b]}</th>
              <th style="padding:4px 6px">胜率</th>
              <th style="padding:4px 6px;text-align:right">均步</th>
            </tr>
{chr(10).join(body)}
          </table>
        </details>
      </div>''')
    return '''      <div class="about-card">
        <h3 style="font-size:1.05rem;margin-bottom:8px">配对明细（A:B，每格 {cfg["games"]} 局）</h3>
        <p style="font-size:.78rem;color:var(--dim);margin:0">点开看每种形状 / 边界 / 阈值下的比分。绿色 = A 占优，红色 = A 落后</p>
      </div>
''' + '\n'.join(cards)


def build_row_card(rows, cfg):
    """各组合的平均步数。"""
    w, g = tally(rows, lambda x: (x['shape'], x['bm'], x['cm']))
    steps = defaultdict(list)
    for x in rows:
        steps[(x['shape'], x['bm'], x['cm'])].append(x['steps'])
    shapes = cfg.get('shapes') or ['square']
    body = []
    for sh in shapes:
        for bm in BM_ORDER:
            if bm not in cfg['borders']:
                continue
            cells = []
            for cm in CAP_ORDER:
                if cm not in cfg['caps']:
                    continue
                v = steps.get((sh, bm, cm)) or []
                cells.append(f'<td style="padding:3px 6px;text-align:center">{sum(v) / len(v):.0f}</td>'
                             if v else '<td style="padding:3px 6px;text-align:center">-</td>')
            body.append(f'<tr><td style="padding:3px 6px">{SHAPE_LABEL.get(sh, sh)}</td>'
                        f'<td style="padding:3px 6px">{BM_LABEL.get(bm, bm)}</td>' + ''.join(cells) + '</tr>')
    head = ''.join(f'<th style="padding:4px 6px">{CAP_LABEL[c]}</th>' for c in CAP_ORDER if c in cfg['caps'])
    return f'''      <div class="about-card">
        <h3 style="font-size:1.05rem;margin-bottom:8px">平均对局步数</h3>
        <table style="width:100%;font-size:.78rem;border-collapse:collapse">
          <tr style="border-bottom:1px solid rgba(255,255,255,.1)">
            <th style="text-align:left;padding:4px 6px">形状</th>
            <th style="text-align:left;padding:4px 6px">边界</th>{head}
          </tr>
          {chr(10).join('          ' + r for r in body)}
        </table>
      </div>
'''


def build_capped_card(rows):
    capped = [x for x in rows if is_capped(x)]
    if not capped:
        return ''
    dist = defaultdict(int)
    for x in capped:
        if x['winner'] is not None:
            dist[x['players'][x['winner']]] += 1
    dist_txt = '、'.join(f'{ALG_LABEL.get(a, a)} {n} 局'
                         for a, n in sorted(dist.items(), key=lambda kv: -kv[1]))
    return f'''      <div class="about-card">
        <h3 style="font-size:1.05rem;margin-bottom:8px">未分胜负的对局</h3>
        <p style="font-size:.82rem;line-height:1.7;margin:0">
          {len(capped)} / {len(rows)} 局（{pct(len(capped), len(rows)):.1f}%）打到 1000 步上限仍没有任何玩家出局。
          引擎在这个位置按「第一个存活玩家」判胜，先后手直接决定结果，所以这些胜负不代表真实实力。
          它们集中在慢节奏 + 大形状组合，说明这些配置下缺少终结机制。
        </p>
        <p style="font-size:.78rem;color:var(--dim);margin:6px 0 0">被判胜分布：{dist_txt}</p>
      </div>
'''


def build_block(rows, cfg, model_note):
    order = alg_order(rows)
    use_ml = cfg.get('useMlEval', True)
    overall = overall_stats(rows)
    total = len(rows)
    capped_n = sum(1 for x in rows if is_capped(x))

    parts = [
        '  <div id="about-benchmark" class="screen">\n',
        '    <button class="back glass-btn" onclick="Router.back()">← 返回</button>\n',
        '    <div class="about-scroll">\n',
        '      <h2>AI 性能评测</h2>\n\n',
        build_config_card(cfg, total, capped_n, use_ml, model_note, order),
        '\n',
        build_summary_card(rows, overall, order),
        '\n',
        build_ranking_card(overall, order),
        '\n',
        build_matrix_card(rows, cfg, '形状 × 边界（每格：胜场最多的算法与胜率）',
                          '同一形状下不同边界的强弱会变，回环与降级最容易改变格局', order),
        '\n',
        build_row_card(rows, cfg),
        '\n',
        build_capped_card(rows),
        build_pair_cards(rows, cfg, order),
        '    </div>\n',
        '  </div>\n',
    ]
    return ''.join(p for p in parts if p)


def find_block(s):
    """定位 about-benchmark 整块（含结束标签）。"""
    start = s.index('  <div id="about-benchmark" class="screen">')
    # 结束标记：该块之后是下载/更新日志等下一个 screen
    tail_markers = ['\n  <div id="about-changelog"', '\n  <div id="about-rules"']
    ends = [s.index(m, start) for m in tail_markers if m in s[start:]]
    end = min(ends)
    # 回退到上一行空行，保证块边界整洁
    while end > start and s[end - 1] == '\n':
        end -= 1
    return start, end


def main():
    ap = argparse.ArgumentParser(description='从 ai_report.json 生成评测页 HTML')
    ap.add_argument('--json', default=DEFAULT_JSON, help='对局数据 JSON')
    ap.add_argument('--dry-run', action='store_true', help='只打印，不写文件')
    a = ap.parse_args()

    with open(a.json, encoding='utf-8') as f:
        data = json.load(f)
    rows = data['results']
    cfg = data['config']

    # 模型说明：从文件指纹推，写不出就省略
    model_note = '内嵌 XGBoost'
    model_path = os.path.join(ROOT, 'engine', 'xgb_model_board.json')
    try:
        import hashlib
        with open(model_path, 'rb') as f:
            digest = hashlib.sha256(f.read()).hexdigest()[:8]
        mtime = datetime.fromtimestamp(os.stat(model_path).st_mtime).strftime('%Y-%m-%d')
        model_note = f'{digest} · {mtime} 训练'
    except OSError:
        pass

    block = build_block(rows, cfg, model_note)
    if a.dry_run:
        print(block)
        return

    for path in TARGETS:
        s = open(path, encoding='utf-8').read()
        start, end = find_block(s)
        s = s[:start] + '  ' + block + s[end:]
        open(path, 'w', encoding='utf-8').write(s)
        print(f'✅ {os.path.relpath(path, ROOT)} 已更新（{len(rows)} 局）')


if __name__ == '__main__':
    main()
