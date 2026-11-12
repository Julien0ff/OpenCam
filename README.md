# 📷 OpenCam

**OpenCam** est une suite de webcam virtuelle open-source et multiplateforme **100% Rust** conçue comme une alternative moderne, sobre et ultra-faible latence (< 30 ms) à iVCam et Camo Studio.

---

## 🌟 Points Forts & Architecture

- **Codebase 100% Rust Unifiée :**
  - **Mobile (Transmetteur) :** Android (via `cargo-ndk` avec APIs C NDK Camera2 et encodage matériel `AMediaCodec`) et iOS (via AVFoundation `objc2`).
  - **Desktop (Régie Multi-Caméras) :** Linux (injection native dans `/dev/videoX` via `v4l2loopback`) et Windows (Media Foundation Virtual Camera Sink).
- **Center Stage & Auto-Framing par Distance :**
  - **Suivi de la tête (Head Tracking) :** Recadre le sujet en continu avec un amortissement doux inspiré d'Apple Center Stage.
  - **Zoom automatique selon la distance :** Compense le recul du sujet pour maintenir un cadrage idéal (Visage, Buste ou Plan Large).
- **Zéro Framework Hybride Lourd :** Pas de Flutter, pas de React Native, pas de runtime Java lourd. Interface native sobre en `egui` (60 FPS, palette sombre Pro Studio).
- **Transport Réseau Ultra-Faible Latence (< 30 ms) :**
  - **Découverte automatique mDNS / DNS-SD :** Annonce du service `_opencam._tcp.local` avec connexion en un clic.
  - **Canal de Contrôle RPC (TCP) :** Pilotage à distance des caméras physiques (avant/arrière/ultra-wide), zoom numérique 1.0x-10.0x, miroir H/V, rotation (0°, 90°, 180°, 270°), torche et suivi de la tête.
  - **Canal Vidéo Paquetisé (UDP) :** Découpage MTU et réassemblage dynamique avec protection *Drop-on-Late* (aucune accumulation de buffer en cas de gigue réseau).

---

## 📂 Structure du Workspace Cargo

```text
├── Cargo.toml                  # Workspace Cargo avec dépendances partagées
├── website/                    # Site vitrine officiel avec simulateur Center Stage interactif
│   └── index.html              # Landing page responsive (Apple HIG & Web Design Guidelines)
├── crates/
│   ├── core-protocol/          # Enums, structs, paquets OCLV, encodage TCP/UDP & Center Stage
│   ├── camera-capture/         # Trait VideoCapture, Android NDK Camera2, iOS, HeadTracker
│   ├── vcam-sink/              # Trait VirtualCameraSink, ioctl Linux v4l2loopback & convertisseur YUYV
│   ├── app-mobile/             # Application smartphone (UI egui, capture NDK, stream UDP)
│   │   └── android/            # AndroidManifest.xml, build-android.sh, configuration NDK
│   └── app-desktop/            # Régie Desktop multi-caméras (UI studio pro, aperçu direct, Center Stage)
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
sudo modprobe v4l2loopback devices=2 video_nr=10,11 card_label="OpenCam 1","OpenCam 2" exclusive_caps=1
```

### 2. Lancer la Régie Desktop
```bash
cargo run --bin app-desktop --release
```

### 3. Lancer le Transmetteur Mobile (Test local ou Smartphone)
Sur votre machine locale :
```bash
cargo run --bin app-mobile --release
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

---

## 📱 Connexion Sans Application (QR Code)
Plus besoin d'installer quoi que ce soit sur votre smartphone :
1. Lancez **OpenCam Studio** sur votre PC (Windows ou Linux).
2. Un **QR Code** s'affiche à l'écran (ou via le bouton `Sans app (QR Code)`).
3. Scannez-le avec l'appareil photo de votre iPhone (Safari) ou smartphone Android (Chrome) sur le même réseau Wi-Fi pour diffuser immédiatement.

---

## 📄 Licence
Ce projet est un logiciel libre distribué sous licence **GNU General Public License v3.0 (GPL-3.0)**. Voir le fichier [`LICENSE`](LICENSE) pour plus de détails.
