#!/usr/bin/env bash
set -e

# ==============================================================================
# OpenCam - Script de Cross-Compilation Android via cargo-ndk
# ==============================================================================

echo "=========================================================="
echo "  OpenCam - Compilation Mobile Android (Full Rust NDK)"
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

# 5. Emballage et signature automatique de l'APK (si Android SDK build-tools et platforms sont présents)
ANDROID_SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
if [ -d "$ANDROID_SDK" ]; then
    BUILD_TOOLS_DIR=$(ls -d "$ANDROID_SDK"/build-tools/* 2>/dev/null | sort -V | tail -n 1)
    PLATFORM_JAR=$(ls -d "$ANDROID_SDK"/platforms/android-*/android.jar 2>/dev/null | sort -V | tail -n 1)

    if [ -n "$BUILD_TOOLS_DIR" ] && [ -n "$PLATFORM_JAR" ]; then
        echo "[*] Génération directe du package APK signé..."
        mkdir -p ~/.android
        if [ ! -f ~/.android/debug.keystore ]; then
            keytool -genkey -v -keystore ~/.android/debug.keystore -alias androiddebugkey -storepass android -keypass android -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US" 2>/dev/null
        fi

        "$BUILD_TOOLS_DIR/aapt2" link -I "$PLATFORM_JAR" --min-sdk-version 24 --target-sdk-version 34 --manifest AndroidManifest.xml -o unaligned.apk
        python3 -c "
import zipfile
with zipfile.ZipFile('unaligned.apk', 'a', compression=zipfile.ZIP_DEFLATED) as zf:
    zf.write('$OUTPUT_DIR/arm64-v8a/libapp_mobile.so', 'lib/arm64-v8a/libapp_mobile.so')
"
        "$BUILD_TOOLS_DIR/zipalign" -v -f 4 unaligned.apk aligned.apk > /dev/null
        "$BUILD_TOOLS_DIR/apksigner" sign --ks ~/.android/debug.keystore --ks-pass pass:android --key-pass pass:android --out opencam-mobile-android-arm64.apk aligned.apk
        rm -f unaligned.apk aligned.apk
        echo "[+] APK généré avec succès : $(pwd)/opencam-mobile-android-arm64.apk"
    fi
fi

echo "[*] Pour déployer sur votre smartphone connecté via USB :"
echo "    adb install -r opencam-mobile-android-arm64.apk"

