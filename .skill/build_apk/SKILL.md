---
name: build-apk
description: Build and sign the chain-chess Android APK on arm64 (aarch64) Linux.
---

# Build APK — arm64 构建指南

当前环境（Debian 13 / arm64）各项依赖已就绪，可直接构建，如果存在build_apk.sh，也可使用。

## 环境配置总览

### Android SDK

```sh
export ANDROID_HOME=/home/ywnh1/Android/Sdk
```

| 组件 | 路径 |
|------|------|
| SDK 根目录 | `$ANDROID_HOME` → `/home/ywnh1/Android/Sdk` |
| Platforms | `$ANDROID_HOME/platforms/android-{33,34,36}` |
| Build Tools | `$ANDROID_HOME/build-tools/{34,35,36}.0.0` |
| NDK | `$ANDROID_HOME/ndk/27.0.12077973`（备用） |
| NDK | `$ANDROID_HOME/ndk/28.2.13676358`（当前使用） |

NDK 28 的 `llvm-readelf` 已替换为系统 aarch64 原生版本（来自 `llvm-19-tools` 包）。

### Java

```sh
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-arm64
export PATH=$JAVA_HOME/bin:$PATH
```

- JDK: OpenJDK 21.0.11 (arm64)
- 路径: `/usr/lib/jvm/java-21-openjdk-arm64`
- java 二进制: `/usr/bin/java` → 指向上述 JDK

### Rust (Android 交叉编译 Targets)

```sh
rustup target add aarch64-linux-android   # ARM64 (主要)
rustup target add armv7-linux-androideabi  # ARM32
rustup target add x86_64-linux-android     # x86_64 模拟器
rustup target add i686-linux-android       # x86 模拟器
```

NDK 通过 QEMU (`/usr/bin/qemu-x86_64` v10.0.11) 模拟 x86_64 工具链：
- `clang-19` / `ld.lld` / `llvm-ar` 等通过 QEMU wrapper 正常
- `llvm-readelf` / `llvm-readobj` 已替换为原生 aarch64 二进制

### Tauri CLI

```sh
# 已安装: tauri-cli 2.11.4
npx tauri --version
```

### 工具链

| 工具 | 版本 |
|------|------|
| apksigner | 0.9 (`/usr/bin/apksigner`) |
| qemu-x86_64 | 10.0.11 |
| gradle | 8.14.3 (wrapper) |
| Keystore | `release.keystore` (密码: `chainchess`) |

## 一键构建

```sh
cd /home/ywnh1/Programs/chain-chess
ANDROID_HOME=/home/ywnh1/Android/Sdk ./build_apk.sh chainchess
```

产物: `release/连锁棋-1.3.2-beta.apk`（约 12MB，v2+v3 签名）。

### 构建脚本 `build_apk.sh` 内部流程

```
1. npx tauri android build --target aarch64
   ├─ cargo build --target aarch64-linux-android (编译 Rust → .so)
   ├─ symlink .so → jniLibs/arm64-v8a/
   └─ gradlew assembleUniversalRelease (编译 Java/Kotlin → APK)
2. apksigner sign --ks release.keystore (签名)
3. apksigner verify (验证)
```

## 完整手动步骤（从零开始）

```sh
# 1. 设置环境变量
export ANDROID_HOME=/home/ywnh1/Android/Sdk
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-arm64

# 2. 定位项目
cd /home/ywnh1/Programs/chain-chess

# 3. 安装 npm 依赖（首次或依赖变更后）
cd tauri && npm install && cd ..

# 4. 初始化 Android 项目（首次）
cd tauri && npx tauri android init && cd ..

#    ⚠️ 初始化后需手动修复:
#    a) 下载 Gradle Wrapper JAR
#       curl -sLo tauri/src-tauri/gen/android/gradle/wrapper/gradle-wrapper.jar \
#         "https://raw.githubusercontent.com/gradle/gradle/v8.14.3/gradle/wrapper/gradle-wrapper.jar"
#    b) 替换 NDK llvm-readelf（arm64 兼容性）
#       cp /usr/bin/llvm-readelf $ANDROID_HOME/ndk/28.2.13676358/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-readelf
#       cp /usr/bin/llvm-readobj $ANDROID_HOME/ndk/28.2.13676358/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-readobj

# 5. 构建 + 签名
ANDROID_HOME=$ANDROID_HOME ./build_apk.sh chainchess
```

## 产物验证

```sh
apksigner verify --verbose release/连锁棋-1.3.2-beta.apk
# 应输出:
#   Verified using v2 scheme: true
#   Verified using v3 scheme: true

ls -lh release/*.apk
# 约 12MB
```

## 常见问题

| 问题 | 原因 | 解决 |
|------|------|------|
| `llvm-readelf: signal 4` | NDK x86_64 二进制在 arm64 QEMU 下崩溃 | 用系统 `/usr/bin/llvm-readelf` 覆盖 |
| `ClassNotFoundException: GradleWrapperMain` | `gradle-wrapper.jar` 缺失 | 从 Gradle GitHub 下载对应版本 |
| `binary not found: clang` | NDK clang QEMU wrapper 找不到二进制 | 检查 `$ANDROID_HOME/qemu-wrappers/clang-19` 是否存在 |
| `openssl` 相关错误 | 交叉编译缺 OpenSSL | `cargo build` 会自动处理，确保 NDK 完整 |
