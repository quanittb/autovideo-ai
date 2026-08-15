# AutoVideo AI — UI Specification

## Visual Direction & Aesthetics

AutoVideo AI implements a dark mode visual hierarchy with HSL-tailored slate surfaces (`#0f172a`, `#1e293b`), vibrant purple/indigo primary accents (`#7c3aed`, `#6366f1`), subtle glassmorphism container borders, and high-legibility typographic hierarchy.

---

## Screen Breakdown & Flow

### 1. Dashboard / Home View (`Welcome Screen`)
- **Left Navigation Sidebar**:
  - Logo & Brand Title ("AutoVideo AI — Desktop Studio")
  - Navigation items: `Home`, `Workspace`, `Projects`, `Jobs & Pipeline`, `AI Models`, `Settings`
  - Bottom section: System Engine status widget (`Local AI Engine: Ready`, GPU device, Full Access Studio indicator).
- **Hero Transformation Card**:
  - Prominent banner featuring interactive before/after visual demonstration ("Fox → Rabbit")
  - "Create Project" primary CTA button launching the transformation workspace
- **Quick Tools Grid**:
  - `Character Replacement` (Replace characters with AI — MVP)
  - `Scene Transformation` (Change scene, season, location with AI)
  - `Style Transfer` (Apply different visual styles)
  - `Video Enhancer` (Improve quality & resolution)
- **Recent Projects & Outputs Gallery**:
  - Card grid showing recent project thumbnails, titles ("Winter to Autumn", "Fox to Rabbit", "Beach Vacation", "Home to Market"), and last modified dates.

---

### 2. Step 1 — Upload Your Video
- **Top Wizard Navigation**:
  - Header: `New Project` breadcrumb, Step Tracker (`(1) Upload` -> `(2) Transform` -> `(3) Processing` -> `(4) Result` -> `(5) Export`), `Next` action button.
- **Left Column**:
  - Interactive Drag & Drop zone supporting MP4, MOV, AVI, MKV (Max 2GB, recommended <3 min).
  - "Tips for better results" card (high quality, good lighting, clear characters).
- **Right Column**:
  - Original video preview player with transport controls.
  - Video Information card (File Name, Duration, Resolution, Size).

---

### 3. Step 2 — Project Workspace & Transform
- **3-Column + Bottom Strip Layout**:
  - **Left**: Project context and active scene summary.
  - **Center**: Large video preview with transport bar and time scrubber.
  - **Right**: AI Transform control panel:
    - Mode tabs: `Character (MVP)`, `Background`, `Environment`, `Style`, `Object`, `Custom`.
    - Character Replacement: Detected subject vs Target subject card, reference image uploader, prompt input, and 4 preservation rules (Motion, Camera, Composition, Original Audio).
    - Primary CTA: `Generate Transformed Video`.
  - **Bottom**: Scene strip for multi-shot navigation without NLE timeline complexity.

---

### 4. Step 3 — Processing & Job Monitor
- 8-stage progress tracker (`Analysis`, `Planning`, `Preparation`, `Transformation`, `Temporal Refinement`, `Audio`, `Quality Check`, `Export`).
- Hardware telemetry (Active GPU, VRAM usage MB, estimated seconds remaining).
- Lifecycle controls: Pause, Resume, Cancel, and Review Transformed Video CTA.

---

### 5. Step 4 — Result Inspection & QC
- 3-mode comparison player (Interactive Split Slider, Side-by-Side, Before/After Toggle).
- Quality Report (Temporal Stability %, Identity Fidelity %, Audio/Video Sync alignment offset ms, lighting warnings).

---

### 6. Step 5 — Export Your Video
- **Left Export Settings Panel**:
  - Resolution dropdown (`1080p (1920x1080)`, `4K (3840x2160)`, `720p`).
  - Quality dropdown (`High Quality`, `Standard`, `Lossless (Master)`).
  - Codec & FPS selectors (`H.264`, `HEVC`, `Apple ProRes` • `24, 30, 60 fps`).
  - Audio track options (`Preserve Original Audio`, `AI Enhanced Audio`).
  - Output folder directory selector.
  - `Export Video File` primary action CTA.
- **Right Export Preview & Summary**:
  - Export preview player.
  - File Output summary cards (Duration, Resolution, Format, Estimated Size).

---

## Complete Studio Access & Honesty Badge Integration

1. **No Monetization Tiers**: AutoVideo AI is a complete desktop studio. All features, resolutions (1080p, 4K), and quality presets are accessible to all users without subscriptions, paywalls, or watermark gating.
2. **Honesty Badges**: Whenever the application displays fallback/sample assets or non-production AI runtime outputs, a distinct visual tag `[DEMO DATA / MOCK]` is rendered in the top-right corner of the player to guarantee compliance with the **NEVER FAKE AI** rule.
