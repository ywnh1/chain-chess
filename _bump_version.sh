#!/bin/sh
set -e
cd /home/ywnh1/Programs/chain-chess

# Update package.json
sed -i 's/"version": "1.2.7"/"version": "1.2.8"/' tauri/package.json

# Update tauri.conf.json
sed -i 's/"version": "2.3.7"/"version": "2.3.8"/' tauri/src-tauri/tauri.conf.json

# Update Android properties
sed -i 's/tauri.android.versionName=2.3.7/tauri.android.versionName=2.3.8/' tauri/src-tauri/gen/android/app/tauri.properties
sed -i 's/tauri.android.versionCode=2003007/tauri.android.versionCode=2003008/' tauri/src-tauri/gen/android/app/tauri.properties

# Update build script
sed -i 's/VERSION="2.3.7-beta"/VERSION="2.3.8-beta"/' build_apk.sh

# Update README
sed -i 's/v2.3.7-beta/v2.3.8-beta/g' README.md

# Update changelog version line
sed -i 's/v2.3.7-beta · 第 16 版/v2.3.8-beta · 第 17 版/' tauri/public/index.html

echo "Done"
