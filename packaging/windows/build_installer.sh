#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
XWIN="${XWIN_DIR:-/home/antigravity/.cache/xwin_desktop}"
CEF_CACHE="${CEF_PATH:-/home/antigravity/.cache/cef_win64}"

export PATH="/home/antigravity/.cargo/bin:$PATH"
export CEF_PATH="${CEF_CACHE}"
mkdir -p "${CEF_PATH}" "${REPO_ROOT}/dist"

TOOLCHAIN_FILE="${CEF_CACHE}/xwin_toolchain.cmake"
cat << EOF > "${TOOLCHAIN_FILE}"
set(CMAKE_SYSTEM_NAME Windows)
set(CMAKE_SYSTEM_PROCESSOR AMD64)
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
set(CMAKE_C_COMPILER "/home/antigravity/.cargo/bin/clang-cl")
set(CMAKE_CXX_COMPILER "/home/antigravity/.cargo/bin/clang-cl")
set(CMAKE_AR "/usr/bin/llvm-lib")
set(CMAKE_LINKER "/usr/bin/lld-link")
set(CMAKE_RC_COMPILER "/usr/bin/llvm-rc")
set(CMAKE_MT "/usr/bin/llvm-mt")
add_link_options(
    "/libpath:${XWIN}/crt/lib/x86_64"
    "/libpath:${XWIN}/sdk/lib/um/x86_64"
    "/libpath:${XWIN}/sdk/lib/ucrt/x86_64"
)
EOF

export CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_msvc="${TOOLCHAIN_FILE}"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER="lld-link"
export CC_x86_64_pc_windows_msvc="clang-cl"
export CXX_x86_64_pc_windows_msvc="clang-cl"
export AR_x86_64_pc_windows_msvc="llvm-lib"
export CFLAGS_x86_64_pc_windows_msvc="--target=x86_64-pc-windows-msvc /D_ALLOW_COMPILER_AND_STL_VERSION_MISMATCH /imsvc ${XWIN}/crt/include /imsvc ${XWIN}/sdk/include/ucrt /imsvc ${XWIN}/sdk/include/um /imsvc ${XWIN}/sdk/include/shared /imsvc ${XWIN}/sdk/include/winrt"
export CXXFLAGS_x86_64_pc_windows_msvc="${CFLAGS_x86_64_pc_windows_msvc}"
export RUSTFLAGS="-C target-feature=+crt-static -L native=${XWIN}/crt/lib/x86_64 -L native=${XWIN}/sdk/lib/um/x86_64 -L native=${XWIN}/sdk/lib/ucrt/x86_64"

echo "==> Cross-compiling Rover 2.0 (Pure Chromium 156 CEF) for Windows x86_64-pc-windows-msvc..."
cargo build --release --target x86_64-pc-windows-msvc --manifest-path "${REPO_ROOT}/Cargo.toml" --bin rover

REL_DIR="${REPO_ROOT}/target/x86_64-pc-windows-msvc/release"
CEF_RUNTIME_DIR="$(find "${CEF_CACHE}" -maxdepth 2 -type d -name 'cef_windows_x86_64' | head -n 1)"
STAGE_DIR="${REPO_ROOT}/dist/stage"
rm -rf "${STAGE_DIR}"
mkdir -p "${STAGE_DIR}/locales"

echo "==> Staging rover.exe and pure Chromium 156 (CEF) runtime from ${CEF_RUNTIME_DIR}..."
cp -f "${REL_DIR}/rover.exe" "${STAGE_DIR}/"
for ext in dll pak dat bin; do
  shopt -s nullglob
  for f in "${CEF_RUNTIME_DIR}"/*.${ext}; do
    cp -f "$f" "${STAGE_DIR}/"
  done
  shopt -u nullglob
done
if [ -f "${CEF_RUNTIME_DIR}/vk_swiftshader_icd.json" ]; then
  cp -f "${CEF_RUNTIME_DIR}/vk_swiftshader_icd.json" "${STAGE_DIR}/"
fi
if [ -d "${CEF_RUNTIME_DIR}/locales" ]; then
  cp -f "${CEF_RUNTIME_DIR}/locales/"*.pak "${STAGE_DIR}/locales/"
fi

# Verify zero Microsoft Edge / WebView2 contamination and valid libcef.dll linkage
if [ -f "${STAGE_DIR}/WebView2Loader.dll" ]; then
  echo "ERROR: WebView2Loader.dll must not be staged!" >&2
  exit 1
fi
if [ ! -f "${STAGE_DIR}/libcef.dll" ]; then
  echo "ERROR: libcef.dll missing from staging directory!" >&2
  exit 1
fi

echo "==> Verifying PE imports of rover.exe..."
IMPORTS="$(x86_64-w64-mingw32-objdump -p "${STAGE_DIR}/rover.exe" | grep 'DLL Name:' || true)"
echo "${IMPORTS}"
if echo "${IMPORTS}" | grep -qi "webview2"; then
  echo "ERROR: rover.exe unexpectedly imports WebView2!" >&2
  exit 1
fi
if ! echo "${IMPORTS}" | grep -qi "libcef.dll"; then
  echo "ERROR: rover.exe does not import libcef.dll!" >&2
  exit 1
fi

echo "==> Packaging native Windows installer RoverSetup-2.0.2.exe with NSIS..."
makensis -V2 "${SCRIPT_DIR}/rover.nsi"

cp -f "${REPO_ROOT}/dist/RoverSetup-2.0.2.exe" "${REPO_ROOT}/../RoverSetup-2.0.2.exe"
ls -lh "${STAGE_DIR}/rover.exe" "${STAGE_DIR}/libcef.dll" "${REPO_ROOT}/dist/RoverSetup-2.0.2.exe"
file "${STAGE_DIR}/rover.exe" "${REPO_ROOT}/dist/RoverSetup-2.0.2.exe"
