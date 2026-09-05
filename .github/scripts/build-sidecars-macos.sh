#!/usr/bin/env bash
# macOS：从源码构建 whisper.cpp CLI + 精简 ffmpeg，产出 Tauri sidecar（ADR-0007 补全）。
#
# 背景（v0.5 设计已核实）：ggml-org/whisper.cpp release 无 macOS 原生预编译
# （仅 xcframework/iOS 向）；BtbN/FFmpeg-Builds 无 mac 资产。故 CI 源码构建。
#
# 用法：build-sidecars-macos.sh <target-triple>   （aarch64-apple-darwin / x86_64-apple-darwin）
# 输出：apps/desktop/src-tauri/binaries/{whisper,ffmpeg}-<triple>
# 依赖：brew（cmake 由 steps 安装）；ffmpeg 源码构建约 3-5 分钟。
set -euo pipefail

TARGET="${1:?usage: build-sidecars-macos.sh <target-triple>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BIN_DIR="$REPO_ROOT/apps/desktop/src-tauri/binaries"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT
mkdir -p "$BIN_DIR"

echo ":: [macos] building whisper.cpp (source)"
cd "$WORK_DIR"
git clone --depth 1 https://github.com/ggml-org/whisper.cpp.git
cd whisper.cpp
cmake -B build -DCMAKE_BUILD_TYPE=Release \
  -DWHISPER_BUILD_TESTS=OFF \
  -DWHISPER_BUILD_SERVER=OFF >/dev/null
# 注：不能关 WHISPER_BUILD_EXAMPLES——whisper-cli 目标位于 examples/，
# 关掉后 make --target whisper-cli 报 No rule to make target（v0.7.0 实测）。
cmake --build build --config Release -j"$(sysctl -n hw.ncpu)" --target whisper-cli >/dev/null
CLI="$(find build -type f -name whisper-cli | head -1)"
[[ -n "$CLI" ]] || { echo "ERROR: whisper-cli not built" >&2; exit 1; }
cp "$CLI" "$BIN_DIR/whisper-$TARGET"
chmod +x "$BIN_DIR/whisper-$TARGET"
# 源码构建默认静态链 whisper/ggml（无伴生 dylib）；如 find 到也一并拷
for lib in build/src/libwhisper*.* build/ggml/src/libggml*.*; do
  [[ -e "$lib" ]] || continue
  case "$lib" in *.dylib|*.so*) cp "$lib" "$BIN_DIR/" ;; esac
done 2>/dev/null || true

echo ":: [macos] building minimal ffmpeg (source, static)"
cd "$WORK_DIR"
curl -fL https://ffmpeg.org/releases/ffmpeg-7.1.tar.xz -o ffmpeg.tar.xz
tar xf ffmpeg.tar.xz
cd ffmpeg-7.1
# 精简：只要 demux/decode 常见容器 + pcm 输出；静态单文件
./configure --prefix="$WORK_DIR/ff-out" \
  --disable-everything --disable-doc --disable-programs --disable-network \
  --enable-ffmpeg --enable-decoder=flac,mp3,pcm_s16le,opus,vorbis \
  --enable-demuxer=flac,mp3,ogg,wav,webm,mov,matroska,aac \
  --enable-encoder=pcm_s16le --enable-muxer=wav \
  --disable-shared --enable-static --enable-protocol=file >/dev/null 2>&1
make -j"$(sysctl -n hw.ncpu)" ffmpeg >/dev/null
cp ./ffmpeg "$BIN_DIR/ffmpeg-$TARGET"
chmod +x "$BIN_DIR/ffmpeg-$TARGET"

echo ":: [macos] sidecars ready"
ls -la "$BIN_DIR"
# 静态性自检：ffmpeg 不应链接外部 libffmpeg
otool -L "$BIN_DIR/ffmpeg-$TARGET" | head -5 || true
