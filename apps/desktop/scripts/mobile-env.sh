# Source before `pnpm tauri android ...`: NDK paths the C deps' CMake builds (opus) need.
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
NDK_DIR="$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)"
export NDK_HOME="$NDK_DIR" ANDROID_NDK="$NDK_DIR" ANDROID_NDK_ROOT="$NDK_DIR"

# whisper-rs-sys picks whether to link ggml-blas by the machine building, not the target: from a Mac
# it asks the Android build for a library that was never built. An empty archive satisfies it.
if [ "$(uname)" = "Darwin" ]; then
  STUB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)/../../target/android-stubs"
  mkdir -p "$STUB_DIR" && printf '!<arch>\n' > "$STUB_DIR/libggml-blas.a"
  # Through RUSTFLAGS: the Tauri CLI sets its own, which would hide a per-target setting.
  case " ${RUSTFLAGS:-} " in *"$STUB_DIR"*) ;; *) export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-L native=$STUB_DIR" ;; esac
fi
