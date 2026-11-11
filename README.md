# 📷 OpenCamLink

**OpenCamLink** est une suite de webcam virtuelle open-source et multiplateforme **100% Rust** conçue comme une alternative moderne, performante et ultra-faible latence (< 30 ms) à iVCam et Camo Studio.

---

## 🌟 Points Forts & Architecture

- **Codebase 100% Rust Unifiée :**
  - **Mobile (Transmetteur) :** Android (via `cargo-ndk` avec APIs C NDK Camera2 et encodage matériel `AMediaCodec`) et iOS (via AVFoundation `objc2`).
  - **Desktop (Régie Multi-Caméras) :** Linux (injection native dans `/dev/videoX` via `v4l2loopback`) et Windows (Media Foundation Virtual Camera Sink).
- **Zéro Framework Hybride Lourd :** Pas de Flutter, pas de React Native, pas de runtime Java lourd. Interface native pure en `egui` (60 FPS, thème sombre studio).
- **Transport Réseau Ultra-Faible Latence (< 30 ms) :**
  - **Découverte automatique mDNS / DNS-SD :** Annonce du service `_opencamlink._tcp.local` avec connexion en un clic.
  - **Canal de Contrôle RPC (TCP) :** Pilotage à distance des caméras physiques (avant/arrière/ultra-wide), zoom numérique 1.0x-10.0x, miroir H/V, rotation (0°, 90°, 180°, 270°), et torche.
  - **Canal Vidéo Paquetisé (UDP) :** Découpage MTU et réassemblage dynamique avec protection *Drop-on-Late* (aucune accumulation de buffer en cas de gigue réseau).

---

## 📂 Structure du Workspace Cargo

```text
├── Cargo.toml                  # Workspace Cargo avec dépendances partagées
├── crates/
│   ├── core-protocol/          # Enums, structs, paquets OCLV, encodage TCP/UDP
│   ├── camera-capture/         # Trait VideoCapture, Android NDK Camera2, iOS, générateur dynamique 60fps
│   ├── vcam-sink/              # Trait VirtualCameraSink, ioctl Linux v4l2loopback & convertisseur YUYV
│   ├── app-mobile/             # Application smartphone (UI egui, capture NDK, stream UDP)
│   │   └── android/            # AndroidManifest.xml, build-android.sh, configuration NDK
│   └── app-desktop/            # Régie Desktop multi-caméras (UI studio, aperçu direct, sliders, vcam dispatch)
└── README_ANDROID.md           # Guide complet de cross-compilation Android avec cargo-ndk
```

---

## 🚀 Démarrage Rapide

### 1. Préparation de la Caméra Virtuelle sous Linux (`v4l2loopback`)
Pour injecter le flux dans Discord, OBS, Zoom, Teams ou Chromium :
```bash
# Installer le module kernel
sudo apt install v4l2loopback-dkms   # Debian/Ubuntu
sudo pacman -S v4l2loopback-dkms     # Arch Linux

# Charger 2 caméras virtuelles
sudo modprobe v4l2loopback devices=2 video_nr=10,11 card_label="OpenCamLink 1","OpenCamLink 2" exclusive_caps=1
```

### 2. Lancer la Régie Desktop
```bash
cargo run --bin app-desktop
```

### 3. Lancer le Transmetteur Mobile (Test local ou Smartphone)
Sur votre machine locale :
```bash
cargo run --bin app-mobile
```
Pour compiler et déployer sur un smartphone Android :
Consultez [README_ANDROID.md](README_ANDROID.md) ou exécutez :
```bash
cargo ndk -t arm64-v8a -o crates/app-mobile/android/app/src/main/jniLibs build --release -p app-mobile
```

### 4. Lancer les Tests Unitaires & d'Intégration
```bash
cargo test --all
```
