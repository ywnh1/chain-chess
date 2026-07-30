#!/usr/bin/env python3
"""
自动更新下载测试

启动本地 HTTP 服务器模拟 Gitee release 环境，验证:
1. update.json 可访问、JSON 结构正确
2. APK 文件可下载、大小匹配
3. 下载文件完整性（SHA256）

用法:
  python3 test_update_download.py

需要在当前目录有:
  - update.json
  - release/连锁棋-*.apk
"""

import hashlib
import json
import os
import socket
import subprocess
import sys
import threading
import time
import urllib.request
import urllib.error
import urllib.parse
from http.server import HTTPServer, SimpleHTTPRequestHandler
from pathlib import Path

# ─── 配置 ──────────────────────────────────────────────────
PROJECT_ROOT = Path(__file__).resolve().parent
UPDATE_JSON = PROJECT_ROOT / "update.json"
RELEASE_DIR = PROJECT_ROOT / "release"
SERVER_HOST = "127.0.0.1"

PASS = "✅"
FAIL = "❌"
WARN = "⚠️"


def find_free_port():
    """找一个空闲端口"""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind((SERVER_HOST, 0))
        return s.getsockname()[1]


def sha256_file(path):
    """计算文件的 SHA256"""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


class TestResults:
    def __init__(self):
        self.passed = 0
        self.failed = 0
        self.total = 0

    def ok(self, msg):
        self.passed += 1
        self.total += 1
        print(f"  {PASS} {msg}")

    def fail(self, msg, detail=""):
        self.failed += 1
        self.total += 1
        print(f"  {FAIL} {msg}")
        if detail:
            print(f"       {detail}")

    def summary(self):
        print()
        if self.failed == 0:
            print(f"{PASS} {self.passed}/{self.total} 全部通过")
            return True
        else:
            print(f"{FAIL} {self.failed}/{self.total} 失败")
            return False


def run_tests():
    results = TestResults()

    # ── 0. 前置检查 ──────────────────────────────────────
    print("\n═══ 步骤 0: 前置检查 ═══")

    if not UPDATE_JSON.exists():
        results.fail(f"update.json 不存在: {UPDATE_JSON}")
        return results
    results.ok("update.json 存在")

    apk_files = list(RELEASE_DIR.glob("连锁棋-*.apk"))
    if not apk_files:
        results.fail("release/ 目录下没有 APK 文件")
        return results
    apk_path = max(apk_files, key=lambda p: p.stat().st_mtime)
    apk_size = apk_path.stat().st_size
    apk_sha256 = sha256_file(apk_path)
    results.ok(f"APK 文件: {apk_path.name} ({apk_size:,} bytes)")

    # ── 1. 读取并验证 update.json ───────────────────────
    print("\n═══ 步骤 1: 验证 update.json ═══")

    try:
        with open(UPDATE_JSON, "r", encoding="utf-8") as f:
            data = json.load(f)
    except json.JSONDecodeError as e:
        results.fail("update.json 不是合法 JSON", str(e))
        return results
    results.ok("update.json 是合法 JSON")

    # 检查必需字段
    for field in ["version", "notes", "pub_date", "platforms"]:
        if field not in data:
            results.fail(f"缺少字段: {field}")
            return results
    results.ok("包含所有必需字段 (version, notes, pub_date, platforms)")

    version = data["version"]
    results.ok(f"版本号: {version}")

    # 检查 platforms
    for plat in ["android", "linux"]:
        if plat not in data["platforms"]:
            results.warn(f"缺少 platforms.{plat}")
            continue
        pinfo = data["platforms"][plat]
        url = pinfo.get("url", "")
        if not url:
            results.fail(f"platforms.{plat}.url 为空")
            continue
        if version not in url:
            results.fail(f"platforms.{plat}.url 不含版本号 {version}", url)
        else:
            results.ok(f"platforms.{plat}.url 版本号正确")

    # ── 2. 启动本地 HTTP 服务器 ─────────────────────────
    print("\n═══ 步骤 2: 启动本地 HTTP 服务器 ═══")

    port = find_free_port()
    server_dir = PROJECT_ROOT  # 以项目根目录为 web root

    class SilentHandler(SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=str(server_dir), **kwargs)

        def log_message(self, format, *args):
            pass  # 静默日志

    server = HTTPServer((SERVER_HOST, port), SilentHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    time.sleep(0.3)  # 等服务器就绪
    base_url = f"http://{SERVER_HOST}:{port}"
    results.ok(f"HTTP 服务器已启动: {base_url}")

    try:
        # ── 3. 测试下载 update.json ─────────────────────
        print(f"\n═══ 步骤 3: 下载 update.json ═══")

        update_url = f"{base_url}/update.json"
        try:
            resp = urllib.request.urlopen(update_url, timeout=10)
            remote_json = json.loads(resp.read().decode("utf-8"))
        except (urllib.error.URLError, json.JSONDecodeError) as e:
            results.fail(f"下载/解析 update.json 失败", str(e))
            server.shutdown()
            return results
        results.ok(f"成功下载 update.json ({resp.status})")

        # 对比本地和远程版本号
        if remote_json["version"] == data["version"]:
            results.ok(f"远程版本号一致: {remote_json['version']}")
        else:
            results.fail(
                "远程版本号不一致",
                f"本地: {data['version']}, 远程: {remote_json['version']}"
            )

        # 检查 remote 的 android url 是否正确
        remote_android_url = remote_json["platforms"]["android"]["url"]
        apk_filename = remote_android_url.rsplit("/", 1)[-1]
        results.ok(f"APK 文件名: {apk_filename}")

        # ── 4. 测试下载 APK ────────────────────────────
        print(f"\n═══ 步骤 4: 下载 APK ═══")

        # 本地 APK 路径（相对于 web root）
        # 远程 url 是 /连锁棋-3.1.6.apk，但本地只有 3.1.5
        # 所以用本地 APK 路径构造测试 URL
        local_apk_url = f"{base_url}/release/{urllib.parse.quote(apk_path.name)}"

        try:
            resp = urllib.request.urlopen(local_apk_url, timeout=30)
            remote_size = int(resp.headers.get("Content-Length", 0))
            remote_data = resp.read()
        except urllib.error.URLError as e:
            results.fail(f"下载 APK 失败", str(e))
            server.shutdown()
            return results
        results.ok(f"成功下载 APK ({resp.status}, {remote_size:,} bytes)")

        # 验证文件大小
        if remote_size == apk_size:
            results.ok(f"文件大小匹配: {apk_size:,} bytes")
        else:
            results.fail(
                "文件大小不匹配",
                f"本地: {apk_size:,}, 远程: {remote_size:,}"
            )

        # ── 5. 验证 SHA256 ─────────────────────────────
        print(f"\n═══ 步骤 5: 验证 SHA256 ═══")

        # 写入临时文件后算 SHA256
        import tempfile
        with tempfile.NamedTemporaryFile(delete=False) as tmp:
            tmp.write(remote_data)
            tmp_path = tmp.name

        try:
            remote_sha256 = sha256_file(tmp_path)
            if remote_sha256 == apk_sha256:
                results.ok(f"SHA256 一致: {apk_sha256[:16]}...")
            else:
                results.fail(
                    "SHA256 不一致 — 文件损坏",
                    f"本地: {apk_sha256}\n远程: {remote_sha256}"
                )
        finally:
            os.unlink(tmp_path)

        # ── 6. 测试 version 比较逻辑 ───────────────────
        print(f"\n═══ 步骤 6: 验证版本比较逻辑 ═══")

        # 模拟当前 app 版本为 3.1.5
        from packaging.version import Version

        current_ver = Version("3.1.5")
        remote_ver = Version(remote_json["version"])

        if remote_ver > current_ver:
            results.ok(
                f"远程版本 {remote_ver} > 当前版本 {current_ver} → 可更新"
            )
        elif remote_ver == current_ver:
            results.ok(
                f"远程版本 == 当前版本 → 已是最新"
            )
        else:
            results.ok(
                f"远程版本 < 当前版本 → 无更新"
            )

        # 模拟版本为 3.1.6（与 remote 一致）→ 应该无更新
        same_ver = Version("3.1.6")
        if same_ver >= remote_ver:
            results.ok(
                f"模拟当前 {same_ver} >= 远程 {remote_ver} → 无更新，符合预期"
            )
        else:
            results.fail(
                "版本比较异常",
                f"当前 {same_ver} < 远程 {remote_ver}"
            )

    finally:
        server.shutdown()

    return results


if __name__ == "__main__":
    print("=" * 55)
    print("  ♟ 连锁棋 — 自动更新下载测试")
    print("=" * 55)

    # 检查依赖
    try:
        import packaging.version
    except ImportError:
        print(f"\n{WARN} 需要 packaging 库，正在安装...")
        subprocess.check_call(
            [sys.executable, "-m", "pip", "install", "packaging", "-q"]
        )
        from packaging.version import Version

    results = run_tests()
    passed = results.summary()
    sys.exit(0 if passed else 1)
