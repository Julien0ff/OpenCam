#!/usr/bin/env bash
set -e

# ==============================================================================
# OpenCamLink - Script de Cross-Compilation Android via cargo-ndk
# ==============================================================================

echo "=========================================================="
echo "  OpenCamLink - Compilation Mobile Android (Full Rust NDK)"
echo "=========================================================="

# 1. Vérification des prérequis
if ! command -v cargo-ndk &> /dev/null; then
    echo "[-] cargo-ndk n'est pas installé."
    echo "[+] Installation via : cargo install cargo-ndk"
    cargo install cargo-ndk
fi

if [ -z "$ANDROID_NDK_HOME" ] && [ -z "$NDK_HOME" ]; then
    echo "[!] Attention : La variable d'environnement ANDROID_NDK_HOME ou NDK_HOME n'est pas définie."
    echo "    Exemple : export ANDROID_NDK_HOME=\$HOME/Android/Sdk/ndk/25.2.9519653"
fi

# 2. Cibles Rust Android supportées
TARGET_ARCH="${1:-aarch64-linux-android}"
echo "[*] Architecture cible : $TARGET_ARCH"

rustup target add "$TARGET_ARCH" 2>/dev/null || true

# 3. Répertoire de sortie des bibliothèques natives (.so)
OUTPUT_DIR="android/app/src/main/jniLibs"
mkdir -p "$OUTPUT_DIR"

# 4. Compilation avec cargo-ndk
echo "[*] Lancement de la compilation avec cargo-ndk..."
cargo ndk \
    -t arm64-v8a \
    -o "$OUTPUT_DIR" \
    build --release --package app-mobile --lib

echo "[+] Compilation native terminée avec succès !"
echo "[+] Fichiers générés dans : $OUTPUT_DIR/arm64-v8a/libapp_mobile.so"
echo ""
echo "[*] Pour déployer sur votre smartphone connecté via USB :"
echo "    adb install -r android/app/build/outputs/apk/release/app-release.apk"
