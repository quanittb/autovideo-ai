# AutoVideo AI — UI Specification

## Visual Direction & Aesthetics

AutoVideo AI implements a dark mode visual hierarchy with HSL-tailored slate surfaces (`#0f172a`, `#1e293b`), vibrant purple/indigo primary accents (`#7c3aed`, `#6366f1`), subtle glassmorphism container borders, and high-legibility typographic hierarchy.

---

## Screen Breakdown & Flow

### 1. Dashboard / Home View (`Welcome Screen`)
- **Left Navigation Sidebar**:
  - Logo & Brand Title ("AI Video Magic" / "AutoVideo AI")
  - Navigation items: `Home`, `Projects`, `Templates`, `AI Tools`, `Assets`, `History`, `Settings`
  - Bottom card: "Upgrade to Pro" callout widget with purple gradient CTA
  - User profile avatar & current plan indicator
- **Hero Transformation Card**:
  - Prominent banner featuring interactive before/after visual demonstration ("Fox → Rabbit")
  - "Try Now" primary CTA button launching the transformation wizard
- **Quick Tools Grid**:
  - `Scene Transformation` (Change scene, season, location with AI)
  - `Character Replacement` (Replace characters with AI)
  - `Style Transfer` (Apply different visual styles)
  - `Video Enhancer` (Improve quality & resolution)
- **Recent Projects Gallery**:
  - Card grid showing recent project thumbnails, titles ("Winter to Autumn", "Fox to Rabbit", "Beach Vacation", "Home to Market"), and last modified dates.

---

### 2. Step 1 — Upload Your Video
- **Top Wizard Navigation**:
  - Header: `New Project` breadcrumb, Step Tracker (`(1) Upload` -> `(2) Transform` -> `(3) Preview` -> `(4) Export`), `Next Step` action button.
- **Left Column**:
  - Interactive Drag & Drop zone supporting MP4, MOV, AVI, MKV (Max 2GB, recommended <3 min).
  - "Tips for better results" card (high quality, good lighting, clear characters).
- **Right Column**:
  - Original video preview player with transport controls.
  - Video Information card (File Name, Duration, Resolution, Size).

---

### 3. Step 2 — Transform Your Video
- **Left Controls Panel**:
  - Category Selection Tabs: `Scene`, `Character`, `Style`, `Advanced`.
  - Character Replacement sub-panel:
    - Side-by-side original character vs replacement thumbnail.
    - `Change Character` primary trigger button.
    - Character Description text area (optional prompt input with character counter).
- **Right Side-by-Side Preview Player**:
  - Interactive Split-Slider comparing `Original` (left) vs `Preview` (right).
  - Scrub bar, play/pause, step frame controls.
  - Disclaimer footer: *"This is a preview. The final result may vary."*

---

### 4. Step 4 — Export Your Video
- **Left Export Settings Panel**:
  - Resolution dropdown (`1080p (1920x1080)`, `4K`, `720p`).
  - Quality dropdown (`High Quality`, `Standard`, `Lossless`).
  - Format & FPS selectors (`MP4`, `30 fps`).
  - "Remove Watermark" toggle switch with Pro badge.
  - `Export Video` primary CTA button with estimated processing time.
- **Right Export Preview & Summary**:
  - Export preview player.
  - Export Information cards (Duration, Resolution, Format, Estimated Size).

---

## Honesty Badge Integration

Whenever the application displays fallback/sample assets or non-production AI runtime outputs, a distinct visual tag `[DEMO DATA / MOCK]` is rendered in the top-right corner of the player to guarantee compliance with the **NEVER FAKE AI** rule.
