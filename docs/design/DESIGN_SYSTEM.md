# OpenCam Design System — « Editorial Daylight & Atmospheric Glass »

> **Guide destiné aux agents IA et développeurs** pour la refonte de `website/index.html` ainsi que des applications natives Rust (`crates/app-desktop/src/ui.rs` et `crates/app-mobile/src/main.rs`).
> Ce document traduit les 3 inspirations visuelles de référence situées dans `docs/design/references/` en tokens précis, règles typographiques, composants et code `egui`.

---

## 1. Analyse des 3 références visuelles (`docs/design/references/`)

| Référence | Fichier | Ce qu'on retient |
| :--- | :--- | :--- |
| **01 — Mindly** | `references/01-mindly.jpg` | Fond **papier chaud** (`#F4F3EF`), grand titre **serif éditorial** centré avec italique expressive (*« A clearer mind / *for a brighter you.* »*), boutons **pilules noires** (`#111111`) à fort contraste, cartes flottantes en **verre dépoli** (`backdrop-filter: blur`) gravitant autour d'un sujet humain central. |
| **02 — Railroad** | `references/02-railroad.jpg` | Métaphore de la **fenêtre / hublot à coins très arrondis** (`28px–36px`) qui cadre un paysage/flux vidéo vivant, barre de navigation flottante en **pilule translucide** avec bordure fine `1px rgba(255,255,255,0.26)`. |
| **03 — Mercury** | `references/03-mercury.jpg` | **Scène plein écran atmosphérique** avec vignettage doux, typographie blanche calme et aérée posée directement sur l'image/vidéo, hiérarchie ultra-sobre (un titre, un sous-titre court, une action primaire), zéro surcharge d'icônes ou de badges techniques. |

---

## 2. Anti-Patterns (« IA Slop » à bannir absolument)

Lors de la modification du site web ou des applications `egui` (`app-desktop`, `app-mobile`), **ne jamais utiliser** :
1. **Le thème sombre générique « cyber / gaming / broadcast 2015 »** : fond `#090a0d` + texte tout en monospace + orange néon `#ff6600` partout.
2. **Les fausses interfaces dessinées en SVG** : pas d'illustrations vectorielles naïves de bonhommes ou de fausses fenêtres « On Air ». Le sujet visuel est toujours **le vrai flux caméra** (ou une vraie vidéo d'appel sur le site).
3. **L'abus de MAJUSCULES et de jargon** : remplacer `SPÉCIFICATIONS TECHNIQUES DE L'ARCHITECTURE` ou `PROGRAM BUS` par des phrases humaines en casse normale (« Pensé pour le direct. », « Caméras »).
4. **Les badges décoratifs inutiles** : pas de `CI : Passing`, `v4l2loopback`, `Zero Cloud` alignés dans des boîtes à bordures grises.
5. **L'arc-en-ciel de couleurs d'état** : une seule couleur d'accentuation sémantique (le rouge **Live** `#E5484D`) réservée exclusivement au signal diffusé à l'antenne.

---

## 3. Tokens de Couleurs (CSS & Rust `egui::Color32`)

L'identité repose sur le contraste entre une **feuille de papier chaud lumineuse** (pour les contrôles, réglages et cartes) et une **fenêtre vidéo sombre encadrée** (pour l'image caméra), complétée par des **pilules de verre translucide** superposées à la vidéo.

### 3.1 Surfaces & Fonds

| Rôle | Token CSS | Hex | Rust `egui::Color32` | Usage |
| :--- | :--- | :--- | :--- | :--- |
| **Papier principal** | `--paper` | `#F4F3EF` | `Color32::from_rgb(244, 243, 239)` | Fond principal de la page et des panneaux latéraux de l'application. |
| **Papier secondaire** | `--paper-2` | `#EBE9E3` | `Color32::from_rgb(235, 233, 227)` | Fond des zones en creux, barres secondaires, états survolés doux. |
| **Carte pure** | `--card` | `#FFFFFF` | `Color32::from_rgb(255, 255, 255)` | Cartes Bento, panneaux de réglages, tuiles de caméras inactives. |
| **Cadre vidéo (Canvas)** | `--canvas` | `#141413` | `Color32::from_rgb(20, 20, 19)` | Fond du moniteur vidéo principal (quand aucune caméra n'est active ou autour du flux 16:9). |
| **Verre sur vidéo** | `--glass-dark-bg` | `rgba(255,255,255,0.12)` | `Color32::from_rgba_unmultiplied(255, 255, 255, 31)` | Pilules flottantes au-dessus du flux vidéo. |
| **Verre sur papier** | `--glass-light-bg` | `rgba(255,255,255,0.72)` | `Color32::from_rgba_unmultiplied(255, 255, 255, 184)` | Cartes flottantes translucides posées à cheval sur la vidéo et le papier. |

### 3.2 Typographie & Lignes

| Rôle | Token CSS | Hex | Contraste sur `--paper` | Rust `egui::Color32` | Usage |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Encre primaire** | `--ink` | `#111111` | **17.6 : 1** (AAA) | `Color32::from_rgb(17, 17, 17)` | Titres, texte principal, boutons primaires (fond noir, texte blanc). |
| **Encre secondaire** | `--ink-2` | `#5B5B58` | **6.3 : 1** (AA) | `Color32::from_rgb(91, 91, 88)` | Descriptions, sous-titres, labels de réglages. |
| **Encre tertiaire** | `--ink-3` | `#6E6E6A` | **4.9 : 1** (AA) | `Color32::from_rgb(110, 110, 106)` | Légendes, chemins de fichiers, métadonnées secondaires. |
| **Filet subtil** | `--line` | `rgba(17,17,17,0.08)` | — | `Color32::from_rgba_unmultiplied(17, 17, 17, 20)` | Bordures de cartes `1px`, séparateurs. |
| **Filet marqué** | `--line-2` | `rgba(17,17,17,0.14)` | — | `Color32::from_rgba_unmultiplied(17, 17, 17, 36)` | Bordures de boutons secondaires, champs de saisie. |

### 3.3 Signaux Sémantiques (Usage strict)

| Rôle | Token CSS | Hex | Rust `egui::Color32` | Règle d'or |
| :--- | :--- | :--- | :--- | :--- |
| **En direct (Live)** | `--live` | `#E5484D` | `Color32::from_rgb(229, 72, 77)` | **Uniquement** pour le point lumineux « En direct » et le liseré de la caméra actuellement diffusée. Jamais sur un bouton standard. |
| **Prêt / Connecté** | `--ready` | `#1F8A5B` | `Color32::from_rgb(31, 138, 91)` | Indicateur discret de connexion réseau Wi-Fi / mDNS établie. |
| **Focus clavier** | `--focus` | `#2F5BFF` | `Color32::from_rgb(47, 91, 255)` | Anneau de focus accessibilité (`2px`). |

---

## 4. Typographie & Échelle

Le système associe une **serif éditoriale humaniste** (pour la personnalité et les grands chiffres) à une **sans-serif géométrique neutre** (pour l'interface et la lecture).

1. **Display / Titres (`--serif`)** : `'Instrument Serif'` (Web) — à charger aussi en `.ttf` dans `egui::FontDefinitions` pour les grands titres d'accueil et les chiffres clés (`< 20 ms`, `1.0×`).
   - *Caractéristique signature* : mêler romain et *italique* dans un même titre (ex. `Vos téléphones, *une vraie régie vidéo.*`).
2. **UI & Corps (`--sans`)** : `'Geist'` ou `'Inter'` / police système (`-apple-system`, `Segoe UI`).
   - Casse : **toujours en casse de phrase** (« Choisissez une caméra », jamais `CHOISISSEZ UNE CAMÉRA`).
3. **Code & Métriques (`--mono`)** : `'JetBrains Mono'` ou `ui-monospace` avec `font-variant-numeric: tabular-nums`.
   - Réservé aux adresses IP (`192.168.1.42:9000`), timecodes (`00:12:43`), et chemins système (`/dev/video10`).

---

## 5. Géométrie, Rayons & Ombres

L'esthétique « Mindly / Railroad » repose sur des rayons généreux et imbriqués de façon concentrique (`R_externe = R_interne + padding`) :

- **Grand conteneur / Fenêtre vidéo principale** : `border-radius: 28px` à `36px` (`CornerRadius::same(24)` dans `egui`).
- **Cartes Bento & Panneaux** : `border-radius: 24px` à `28px` (`CornerRadius::same(18)` dans `egui`), bordure `1px solid var(--line)`.
- **Boutons, Nav, Badges et Actions** : **pilule complète** `border-radius: 999px` (`CornerRadius::same(255)` dans `egui`).
- **Ombre portée des éléments flottants** :
  ```css
  box-shadow:
    0 1px 0 rgba(255, 255, 255, 0.9) inset,
    0 24px 48px -24px rgba(25, 25, 35, 0.22),
    0 2px 6px rgba(25, 25, 35, 0.05);
  ```

---

## 6. Architecture de l'Animation au Scroll (Web)

Le site `website/index.html` met en scène la transformation d'un appel vidéo plein écran en une fenêtre de régie maîtrisée :

1. **Acte I — Plein écran immersif (Hero, `0 → 30vh`)** :
   - La vidéo d'appel tourne en fond plein écran (`100vw × 100vh`, `clip-path: inset(0 round 0)`), adoucie par un voile atmosphérique radial (style *Mercury*).
   - Le titre en *Instrument Serif* flotte au centre, encadré par la barre de navigation en pilule de verre.
2. **Acte II — Le hublot et les cartes flottantes (Scène sticky, `30vh → 340vh`)** :
   - Au scroll, `clip-path` referme progressivement la vidéo en une **fenêtre centrée aux coins arrondis (`32px`)** posée sur le fond papier `#F4F3EF` (style *Railroad*).
   - Les 4 coins d'un viseur photographique apparaissent dans l'image, et **4 cartes en verre dépoli** (`backdrop-filter: blur(20px)`) émergent en cascade autour du cadre (style *Mindly*).
   - Sous la fenêtre, trois phrases courtes en *Instrument Serif* se succèdent en fondu.
3. **Acte III — La feuille éditoriale (`#sheet`)** :
   - Une feuille de papier `#F4F3EF` à coins supérieurs arrondis (`40px`) monte par-dessus la scène avec les 3 étapes numérotées, le Bento technique, le comparatif épuré et les téléchargements.
4. **Acte IV — Finale plein écran** :
   - En arrivant en bas de page, le `clip-path` se rouvre en plein écran pour l'appel à l'action final (« *Silence, on tourne.* »).

> **Accessibilité obligatoire** :
> - Bouton Pause/Lecture accessible au clavier pour arrêter la vidéo de fond à tout moment.
> - Si `prefers-reduced-motion: reduce` est activé, la vidéo ne démarre pas automatiquement et les révélations s'affichent sans translation ni flou.

---

## 7. Directives de refonte pour les applications Rust (`egui`)

Lorsqu'un agent IA refond `crates/app-desktop/src/ui.rs` et `crates/app-mobile/src/main.rs`, il doit appliquer les changements suivants :

### 7.1 Configuration globale `egui::Visuals` (Thème Papier Chaud)

Remplacer le thème sombre `BG_STUDIO` / `STUDIO_ORANGE` par cette fonction d'initialisation :

```rust
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, Style, Visuals, epaint::tessellation::StrokeKind};

pub const PAPER: Color32 = Color32::from_rgb(244, 243, 239);      // #F4F3EF
pub const PAPER_SUB: Color32 = Color32::from_rgb(235, 233, 227);  // #EBE9E3
pub const CARD: Color32 = Color32::from_rgb(255, 255, 255);       // #FFFFFF
pub const CANVAS_DARK: Color32 = Color32::from_rgb(20, 20, 19);   // #141413
pub const INK: Color32 = Color32::from_rgb(17, 17, 17);           // #111111
pub const INK_2: Color32 = Color32::from_rgb(91, 91, 88);         // #5B5B58
pub const INK_3: Color32 = Color32::from_rgb(110, 110, 106);      // #6E6E6A
pub const LINE: Color32 = Color32::from_rgba_unmultiplied(17, 17, 17, 22);
pub const LIVE_RED: Color32 = Color32::from_rgb(229, 72, 77);     // #E5484D
pub const READY_GREEN: Color32 = Color32::from_rgb(31, 138, 91);  // #1F8A5B

pub fn apply_editorial_theme(ctx: &egui::Context) {
    let mut style = Style::default();
    let mut visuals = Visuals::light();

    visuals.panel_fill = PAPER;
    visuals.window_fill = CARD;
    visuals.override_text_color = Some(INK);
    visuals.window_corner_radius = CornerRadius::same(20);
    visuals.window_stroke = Stroke::new(1.0, LINE);
    visuals.window_shadow = Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(20, 20, 28, 28),
    };

    // Cartes et conteneurs au repos
    visuals.widgets.noninteractive.bg_fill = CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(16);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK);

    // Boutons secondaires (pilules claires)
    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(255);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, INK);

    // Survol
    visuals.widgets.hovered.bg_fill = PAPER_SUB;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(255);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, INK);

    // Action active / sélection (pilule noire #111111 façon Mindly)
    visuals.widgets.active.bg_fill = INK;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.active.corner_radius = CornerRadius::same(255);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, CARD);
    visuals.selection.bg_fill = INK;
    visuals.selection.stroke = Stroke::new(1.0, CARD);

    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 9.0);
    style.visuals = visuals;
    ctx.set_style(style);
}
```

### 7.2 Refonte de `app-desktop` (`crates/app-desktop/src/ui.rs`)

1. **Barre supérieure calme (Navigation en pilule)** :
   - Supprimer la bande `TOP BROADCAST HEADER` surchargée et le gros badge `OC` orange.
   - Afficher à gauche le nom **OpenCam** avec une simple pastille : grise au repos, rouge `#E5484D` (« En direct ») uniquement quand `vcam_enabled` est actif.
   - Au centre, une **pilule flottante** (`#FFFFFF`, bordure `LINE`, rayon `255`) contenant le sélecteur de vue (`Moniteur simple` / `Double vue`) et les raccourcis caméras `1`, `2`, `3`.
   - À droite, le bouton primaire en **pilule noire `#111111`** : `Diffuser vers la webcam` (ou `Arrêter la diffusion` avec un point rouge).

2. **Le Moniteur Central (La « Fenêtre Railroad »)** :
   - Entourer la zone vidéo centrale d'une marge généreuse (`24px`) sur le fond papier `#F4F3EF`.
   - Dessiner le flux caméra dans un cadre `#141413` à **coins très arrondis (`CornerRadius::same(24)`)**.
   - Superposer aux coins de la vidéo des **pilules translucides sombres** (`Color32::from_rgba_unmultiplied(20, 20, 22, 170)`) au lieu de bandeaux opaques :
     - En haut à gauche : point rouge + nom du téléphone (`Pixel 8 · Visage`).
     - En bas à droite : résolution et latence (`1080p · 14 ms`).
   - Remplacer les grilles `Safe Zones` jaunes/vertes agressives par un **viseur photographique minimal** : 4 petits angles blancs à `70%` d'opacité dans les coins du sujet suivi.

3. **L'Inspecteur Latéral (Cartes Bento `#FFFFFF`)** :
   - Remplacer les accordéons sombres `RACK_BG` par 3 cartes blanches `#FFFFFF` (`CornerRadius::same(18)`, marge interne `18px`, bordure `1px LINE`) :
     1. **Cadrage** : 3 boutons pilules (`Visage`, `Buste`, `Large`) + curseur de souplesse du suivi.
     2. **Optique** : choix du capteur (`Arrière`, `Avant`, `Ultra grand-angle`), curseur de zoom `1.0× – 10.0×`, boutons pilules `Miroir` et `Rotation`.
     3. **Sortie système** : indique sobrement `/dev/video10` ou `Media Foundation` et le débit en Mo/s.

### 7.3 Refonte de `app-mobile` (`crates/app-mobile/src/main.rs`)

1. **Écran d'accueil & connexion (Style Mindly)** :
   - Fond papier `#F4F3EF`, titre centré « **OpenCam** » avec un sous-titre clair (« Choisissez votre ordinateur pour démarrer la caméra. »).
   - Chaque ordinateur découvert en mDNS s'affiche comme une **carte blanche `#FFFFFF` arrondie (`20px`)** avec le nom de l'ordinateur à gauche et un bouton pilule noir `Connecter →` à droite.
2. **Mode caméra active (Style Fenêtre plein écran)** :
   - Dès que le téléphone diffuse, l'aperçu caméra occupe tout l'écran avec des coins arrondis et seulement **deux pilules en verre translucide** :
     - En haut : point rouge + `En direct vers [Nom du PC] · 16 ms`.
     - En bas : pilule de contrôle rapide (`Changer d'objectif`, `Torche`, `Déconnecter`).

