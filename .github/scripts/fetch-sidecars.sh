#!/usr/bin/env bash
# 下载 whisper.cpp + ffmpeg 预编译二进制并放到 Tauri sidecar 目录（ADR-0007）。
#
# 用法：fetch-sidecars.sh <target-triple>
#   target-triple 形如 x86_64-pc-windows-msvc / x86_64-unknown-linux-gnu
#
# 输出：apps/desktop/src-tauri/binaries/<name>-<target-triple>[.exe]
# 失败即 exit 1（release 不应静默缺 sidecar）。
#
# 二进制来源：
#   whisper.cpp: https://github.com/ggerganov/whisper.cpp/releases （whisper-bin-x64.zip / .tar.gz）
#   ffmpeg:      Win 用 BtbN/FFmpeg-Builds；Linux 用 johnvansickle/ffmpeg 静态构建
#
# 注意：具体 release tag/asset 名称可能随上游变化，需定期更新本脚本。
set -euo pipefail

TARGET="${1:?usage: fetch-sidecars.sh <target-triple>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BIN_DIR="$REPO_ROOT/apps/desktop/src-tauri/binaries"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

mkdir -p "$BIN_DIR"

case "$TARGET" in
  x86_64-pc-windows-msvc)
    EXT=".exe"
    # whisper.cpp Windows 预编译：whisper-bin-x64.zip（含 whisper.exe + dll 们）
    WHISPER_URL="https://github.com/ggerganov/whisper.cpp/releases/download/v1.7.1/whisper-bin-x64.zip"
    # ffmpeg Windows：BtbN 的 lgpl-shared x86_64 build
    FFMPEG_URL="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-lgpl.zip"
    ;;
  x86_64-unknown-linux-gnu)
    EXT=""
    WHISPER_URL="https://github.com/ggerganov/whisper.cpp/releases/download/v1.7.1/whisper-bin-x64.tar.gz"
    FFMPEG_URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"
    ;;
  *)
    echo "ERROR: unsupported target $TARGET (only x86_64 Win/Linux supported for now)" >&2
    exit 1
    ;;
esac

echo ":: Fetching whisper.cpp for $TARGET"
cd "$WORK_DIR"
if [[ "$TARGET" == *windows* ]]; then
  curl -fL "$WHISPER_URL" -o whisper.zip
  unzip -q whisper.zip -d whisper-extracted || { echo "unzip failed; install unzip or check zip"; exit 1; }
  # whisper-bin-x64.zip 内主程序路径不定，按名查找
  WHISPER_BIN="$(find whisper-extracted -name 'whisper.exe' -o -name 'main.exe' | head -1)"
else
  curl -fL "$WHISPER_URL" -o whisper.tar.gz
  tar xzf whisper.tar.gz
  WHISPER_BIN="$(find . -type f -name 'whisper' -o -type f -name 'main' | head -1)"
fi
[[ -n "$WHISPER_BIN" ]] || { echo "ERROR: whisper binary not found in archive"; exit 1; }
cp "$WHISPER_BIN" "$BIN_DIR/whisper-$TARGET$EXT"
chmod +x "$BIN_DIR/whisper-$TARGET$EXT" 2>/dev/null || true

echo ":: Fetching ffmpeg for $TARGET"
cd "$WORK_DIR"
if [[ "$TARGET" == *windows* ]]; then
  curl -fL "$FFMPEG_URL" -o ffmpeg.zip
  unzip -q ffmpeg.zip -d ffmpeg-extracted
  FFMPEG_BIN="$(find ffmpeg-extracted -name 'ffmpeg.exe' | head -1)"
else
  curl -fL "$FFMPEG_URL" -o ffmpeg.tar.xz
  tar xf ffmpeg.tar.xz
  FFMPEG_BIN="$(find . -type f -name 'ffmpeg' | head -1)"
fi
[[ -n "$FFMPEG_BIN" ]] || { echo "ERROR: ffmpeg binary not found in archive"; exit 1; }
cp "$FFMPEG_BIN" "$BIN_DIR/ffmpeg-$TARGET$EXT"
chmod +x "$BIN_DIR/ffmpeg-$TARGET$EXT" 2>/dev/null || true

echo ":: Sidecars ready in $BIN_DIR"
ls -la "$BIN_DIR"
