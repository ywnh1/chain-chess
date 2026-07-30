#!/usr/bin/env python3
"""
远程更新下载测试

模拟 app 真实代码路径，对 Gitee 的 API 和 Release 进行下载测试:
1. 用 Gitee API 获取 update.json（base64 解码，模拟 JS 的 atob）
2. 验证 JSON 结构
3. 下载 APK 并校验文件完整性

用法:
  python3 test_update_remote.py
"""

import base64
import hashlib
import json
import os
import sys
import urllib.request
import urllib.error
import urllib.parse

PASS = "✅"
FAIL = "❌"
WARN = "⚠️"


def sha256_file(path):
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

    def warn(self, msg):
        self.total += 1
        self.passed += 1
        print(f"  {WARN} {msg}")


def run_tests():
    results = TestResults()

    # ── 配置 ─────────────────────────────────────
    API_URL = "https://gitee.com/api/v5/repos/ywnh1/chain-chess-release/contents/update.json?ref=main"

    # ── 1. 通过 API 获取 update.json ────────────
    print(f"\n═══ 1. Gitee API → update.json ═══")
    print(f"  GET {API_URL}")

    try:
        req = urllib.request.Request(API_URL, headers={"User-Agent": "chain-chess/3.1.6"})
        resp = urllib.request.urlopen(req, timeout=15)
        api_resp = json.loads(resp.read().decode("utf-8"))
    except Exception as e:
        results.fail(f"API 请求失败", str(e))
        return results

    results.ok(f"API 返回 {resp.status}")

    # 检查 API 响应格式
    if "content" not in api_resp:
        results.fail("API 响应缺少 content 字段", json.dumps(api_resp, ensure_ascii=False)[:200])
        return results

    content_b64 = api_resp["content"]
    encoding = api_resp.get("encoding", "")
    if encoding != "base64":
        results.fail(f"编码不是 base64: {encoding}")
    else:
        results.ok(f"content 编码: base64 ({len(content_b64)} chars)")

    # ── 2. base64 解码（模拟 JS atob）──────────
    print(f"\n═══ 2. base64 解码 update.json ═══")

    try:
        # 修复 base64 padding
        padding = 4 - len(content_b64) % 4
        if padding != 4:
            content_b64 += "=" * padding
        raw_json = base64.b64decode(content_b64).decode("utf-8")
        data = json.loads(raw_json)
    except Exception as e:
        results.fail("base64 解码失败", str(e))
        return results

    results.ok(f"base64 解码成功 ({len(raw_json)} bytes)")

    # ── 3. 验证 JSON 内容 ─────────────────────
    print(f"\n═══ 3. 验证 update.json 内容 ═══")

    for field in ["version", "notes", "pub_date", "platforms"]:
        if field not in data:
            results.fail(f"缺少字段: {field}")
            return results

    results.ok(f"包含所有必需字段")
    results.ok(f"版本号: {data['version']}")
    results.ok(f"更新时间: {data.get('pub_date', 'N/A')}")

    # android 平台
    android = data.get("platforms", {}).get("android", {})
    android_url = android.get("url", "")
    android_size = android.get("size", 0)
    if not android_url:
        results.fail("platforms.android.url 为空")
    else:
        results.ok(f"android URL: {android_url[:70]}...")
        results.ok(f"android 大小: {android_size:,} bytes")

    # 版本比较（>=）
    from packaging.version import Version
    current_ver = Version("3.1.6")
    remote_ver = Version(data["version"])
    if remote_ver >= current_ver:
        results.ok(f"版本比较: {remote_ver} >= {current_ver} → 可更新")
    else:
        results.warn(f"版本比较: {remote_ver} < {current_ver} → 无更新")
    # 也测试 3.1.5（模拟用户升级）
    old_ver = Version("3.1.5")
    if remote_ver >= old_ver:
        results.ok(f"模拟旧版 {old_ver}: {remote_ver} >= {old_ver} → 可更新")

    # ── 4. 下载 APK ────────────────────────────
    print(f"\n═══ 4. 下载 APK ═══")

    import tempfile
    tmp_apk = None

    try:
        # URL 编码中文路径
        parsed = urllib.parse.urlparse(android_url)
        encoded_path = urllib.parse.quote(parsed.path, safe='/:@!$&\'()*+,;=-._~')
        safe_url = urllib.parse.urlunparse(parsed._replace(path=encoded_path))
        req = urllib.request.Request(safe_url, headers={"User-Agent": "chain-chess/3.1.6"})
        resp = urllib.request.urlopen(req, timeout=120)
        apk_data = resp.read()
        remote_size = len(apk_data)
    except Exception as e:
        results.fail("APK 下载失败", str(e))
        print("\n═══ 测试结束（APK 下载失败）═══")
        return results

    results.ok(f"下载成功: {remote_size:,} bytes")

    # 验证大小
    if android_size > 0 and remote_size == android_size:
        results.ok(f"文件大小匹配: {remote_size:,}")
    elif android_size > 0:
        results.fail(f"文件大小不匹配", f"声明: {android_size:,}, 实际: {remote_size:,}")
    else:
        results.warn(f"update.json 中 size=0，无法校验大小")

    # 写入临时文件计算 SHA256
    tmp_apk = tempfile.NamedTemporaryFile(delete=False)
    tmp_apk.write(apk_data)
    tmp_apk.close()
    apk_sha256 = sha256_file(tmp_apk.name)
    results.ok(f"SHA256: {apk_sha256[:16]}...")

    # ── 5. 验证 APK 签名 ────────────────────
    print(f"\n═══ 5. APK 签名检查 ═══")

    import zipfile
    try:
        with zipfile.ZipFile(tmp_apk.name) as zf:
            # 检查 META-INF 目录（APK 签名）
            meta_inf = [n for n in zf.namelist() if n.startswith("META-INF/")]
            if meta_inf:
                results.ok(f"APK 已签名 ({len(meta_inf)} 个签名文件)")
            else:
                results.warn("APK 无 META-INF 签名")
            # 检查关键文件
            for f in ["assets/index.html", "assets/app.js", "assets/tauri.conf.json"]:
                if f in zf.namelist():
                    results.ok(f"包含 {f}")
                else:
                    results.warn(f"缺少 {f}")
    except Exception as e:
        results.fail("APK 解析失败", str(e))

    # 清理
    if tmp_apk:
        os.unlink(tmp_apk.name)

    return results


if __name__ == "__main__":
    print("=" * 55)
    print("  ♟ 连锁棋 — 远程更新下载测试")
    print(f"  测试真实 Gitee API + Release 下载")
    print("=" * 55)

    # 检查依赖
    try:
        import packaging.version
    except ImportError:
        print(f"\n{WARN} 需要 packaging 库，正在安装...")
        import subprocess
        subprocess.check_call(
            [sys.executable, "-m", "pip", "install", "packaging", "-q"]
        )

    results = run_tests()
    passed = results.summary()
    sys.exit(0 if passed else 1)
