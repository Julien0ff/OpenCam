# OpenCam - Guide de Cross-Compilation Android (Full Rust NDK)

Ce guide détaille la procédure complète pour compiler l'application mobile **OpenCam** pour les smartphones Android en utilisant la chaîne d'outils **Rust NDK** (`cargo-ndk`), sans framework hybride (zéro Flutter, zéro React Native, zéro Java lourd).

---

## 1. Architecture Mobile Android

- **Capture Caméra :** Utilisation directe des APIs C du NDK (`camera2_ndk.h`) via `ndk-sys`.
- **Encodage Matériel :** Encodage H.264 / HEVC matériel à faible consommation d'énergie via `AMediaCodec`.
- **Interface Utilisateur :** UI réactive développée en Rust pur avec `egui` (thème studio sombre, affichage du viseur, contrôles tactiles).
- **Transport Réseau :**
  - **Découverte :** mDNS / DNS-SD (`mdns-sd`) pour détecter automatiquement la régie PC sur le Wi-Fi local.
  - **Canal de Contrôle RPC (TCP) :** Réception des commandes de zoom, miroir, rotation, sélection de capteur et torche.
  - **Canal Vidéo (UDP) :** Flux temps réel paquetisé avec mécanisme *Drop-on-Late* pour garantir une latence inférieure à 30 ms.

---

## 2. Prérequis sur la Machine Hôte

### A. Chaîne d'outils Rust & Cibles Android
Ajoutez les cibles de compilation croisée Android :
```bash
# Architecture 64 bits moderne (la quasi-totalité des smartphones actuels)
rustup target add aarch64-linux-android

# Architecture 32 bits (appareils plus anciens)
rustup target add armv7-linux-androideabi

# Émulateur Android x86_64
rustup target add x86_64-linux-android
```

### B. Installer `cargo-ndk`
`cargo-ndk` gère automatiquement la configuration des compilateurs C du NDK (`clang`), les sysroots, et le placement des bibliothèques `.so` :
```bash
cargo install cargo-ndk
```

### C. Android SDK & NDK
Téléchargez Android Studio ou installez le NDK standalone (version r25c ou supérieure recommandée) :
```bash
# Définir la variable d'environnement vers votre NDK
export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/25.2.9519653
# ou
export NDK_HOME=$HOME/Android/Sdk/ndk/25.2.9519653
```

---

## 3. Compilation de la Bibliothèque Native (`.so`)

Pour compiler pour un smartphone physique standard (ARM64) :
```bash
cargo ndk -t arm64-v8a -o crates/app-mobile/android/app/src/main/jniLibs build --release -p app-mobile
```

Le fichier `libapp_mobile.so` sera généré et placé automatiquement dans :
`crates/app-mobile/android/app/src/main/jniLibs/arm64-v8a/libapp_mobile.so`.

---

## 4. Packaging de l'APK

Deux options sont possibles pour créer l'APK final :

### Option A : Via Gradle (Standard)
1. Ouvrez le dossier `crates/app-mobile/android` dans Android Studio ou utilisez le wrapper Gradle :
   ```bash
   cd crates/app-mobile/android
   ./gradlew assembleRelease
   ```
2. Installez l'APK sur votre smartphone connecté en USB :
   ```bash
   adb install -r app/build/outputs/apk/release/app-release.apk
   ```

### Option B : Standalone via `cargo-apk`
Vous pouvez également utiliser `cargo-apk` pour packager directement sans passer par Gradle :
```bash
cargo install cargo-apk
cargo apk build -p app-mobile --release
cargo apk run -p app-mobile
```

---

## 5. Dépannage & Logs en direct

Pour visualiser les logs de la caméra NDK et du réseau en direct sur votre téléphone :
```bash
adb logcat -s RustAndroid OpenCamLink
```

### Permissions requises sur le téléphone
Lors du premier lancement, accordez les permissions :
- **Appareil photo / Caméra**
- **Réseau local** (pour la découverte mDNS)
- **Désactiver l'optimisation de batterie** (recommandé pour éviter les coupures d'écran pendant un stream prolongé).

