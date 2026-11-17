# Source before `pnpm tauri android ...`: NDK paths the C deps' CMake builds (opus) need.
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
NDK_DIR="$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)"
export NDK_HOME="$NDK_DIR" ANDROID_NDK="$NDK_DIR" ANDROID_NDK_ROOT="$NDK_DIR"
