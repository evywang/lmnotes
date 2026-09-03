#!/usr/bin/env bash
# 下载 whisper.cpp + ffmpeg 预编译二进制并放到 Tauri sidecar 目录（ADR-0007）。
#
# 用法：fetch-sidecars.sh <target-triple>
#   target-triple 形如 x86_64-pc-windows-msvc / x86_64-unknown-linux-gnu
#
# 输出：apps/desktop/src-tauri/binaries/<name>-<target-triple>[.exe]（+ Windows 伴生 DLL）
# 失败即 exit 1（release 不应静默缺 sidecar）。
#
# 二进制来源（2026-08 实测）：
#   whisper.cpp: ggml-org/whisper.cpp 最新 release（仓库已从 ggerganov 迁至 ggml-org；
#                旧 v1.7.1 无预编译资产）。CLI 名为 whisper-cli(.exe)（旧版叫 main）。
#                Windows zip 为动态链接构建：whisper-cli.exe 依赖 whisper.dll/ggml*.dll。
#   ffmpeg:      Win 用 BtbN/FFmpeg-Builds win64-lgpl（静态单文件，无 DLL 依赖）；
#                Linux 用 johnvansickle 静态构建。
#
# 优先用 gh release download（走 API，认证 + 稳定；GitHub runner 自带 gh）。
# 注意：上游资产名可能随版本变化，失败时先 gh release view 核对资产清单。
set -euo pipefail

TARGET="${1:?usage: fetch-sidecars.sh <target-triple>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BIN_DIR="$REPO_ROOT/apps/desktop/src-tauri/binaries"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

mkdir -p "$BIN_DIR"

# 解析 whisper.cpp 最新 release tag（资产名随版本号走，但模式固定 whisper-bin-*）
WHISPER_TAG="$(gh release view --repo ggml-org/whisper.cpp --json tagName --jq .tagName)"
echo ":: whisper.cpp latest tag: $WHISPER_TAG"

case "$TARGET" in
  x86_64-pc-windows-msvc)
    EXT=".exe"
    WHISPER_ASSET="whisper-bin-x64.zip"
    WHISPER_BIN_NAME="whisper-cli.exe"
    FFMPEG_REPO="BtbN/FFmpeg-Builds"
    FFMPEG_ASSET="ffmpeg-master-latest-win64-lgpl.zip"
    FFMPEG_BIN_NAME="ffmpeg.exe"
    ;;
  x86_64-unknown-linux-gnu)
    EXT=""
    WHISPER_ASSET="whisper-bin-ubuntu-x64.tar.gz"
    WHISPER_BIN_NAME="whisper-cli"
    FFMPEG_URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"
    ;;
  *)
    echo "ERROR: unsupported target $TARGET (only x86_64 Win/Linux supported for now)" >&2
    exit 1
    ;;
esac

# ── whisper.cpp ──────────────────────────────────────────────────────────────
echo ":: Fetching whisper.cpp ($WHISPER_ASSET) for $TARGET"
cd "$WORK_DIR"
gh release download "$WHISPER_TAG" --repo ggml-org/whisper.cpp \
  --pattern "$WHISPER_ASSET" --dir . --clobber

if [[ -n "$EXT" ]]; then
  unzip -q "$WHISPER_ASSET" -d whisper-extracted
else
  mkdir -p whisper-extracted && tar xzf "$WHISPER_ASSET" -C whisper-extracted
fi

# CLI 在归档内的 Release/（或 bin/）子目录，按名定位
WHISPER_BIN="$(find whisper-extracted -type f -name "$WHISPER_BIN_NAME" | head -1)"
[[ -n "$WHISPER_BIN" ]] || { echo "ERROR: $WHISPER_BIN_NAME not found in archive" >&2; exit 1; }
cp "$WHISPER_BIN" "$BIN_DIR/whisper-$TARGET$EXT"
chmod +x "$BIN_DIR/whisper-$TARGET$EXT"

# 动态构建的伴生库（Windows: whisper.dll/ggml*.dll；Linux: libwhisper.so/libggml*.so）。
# externalBin 只打包命名可执行文件；伴生库须随 bundle.resources 落到安装目录
# （与 sidecar 同目录）才能被动态链接器命中（见 release.yml 的 TAURI_CONFIG 注入）。
WHISPER_DIR="$(dirname "$WHISPER_BIN")"
LIB_GLOB_WIN=("$WHISPER_DIR"/whisper*.dll "$WHISPER_DIR"/ggml*.dll)
LIB_GLOB_LINUX=("$WHISPER_DIR"/libwhisper*.so* "$WHISPER_DIR"/libggml*.so*)
LIB_COUNT=0
for lib in "${LIB_GLOB_WIN[@]}" "${LIB_GLOB_LINUX[@]}"; do
  [[ -e "$lib" ]] || continue
  cp "$lib" "$BIN_DIR/"
  LIB_COUNT=$((LIB_COUNT + 1))
done
if [[ "$LIB_COUNT" -eq 0 ]]; then
  echo ":: WARNING: no companion libs beside whisper-cli (static build?) — verify the packaged exe starts on a clean machine"
else
  echo ":: Copied $LIB_COUNT companion lib(s)"
fi

# ── ffmpeg ──────────────────────────────────────────────────────────────────
echo ":: Fetching ffmpeg for $TARGET"
cd "$WORK_DIR"
if [[ -n "$EXT" ]]; then
  gh release download --repo "$FFMPEG_REPO" --pattern "$FFMPEG_ASSET" --dir . --clobber
  unzip -q "$FFMPEG_ASSET" -d ffmpeg-extracted
  FFMPEG_BIN="$(find ffmpeg-extracted -type f -name "$FFMPEG_BIN_NAME" | head -1)"
else
  curl -fL "$FFMPEG_URL" -o ffmpeg.tar.xz
  tar xf ffmpeg.tar.xz
  FFMPEG_BIN="$(find . -type f -name ffmpeg -perm -u+x | head -1)"
fi
[[ -n "$FFMPEG_BIN" ]] || { echo "ERROR: ffmpeg binary not found in archive" >&2; exit 1; }
cp "$FFMPEG_BIN" "$BIN_DIR/ffmpeg-$TARGET$EXT"
chmod +x "$BIN_DIR/ffmpeg-$TARGET$EXT" 2>/dev/null || true

echo ":: Sidecars ready in $BIN_DIR"
ls -la "$BIN_DIR"
