# Companion skins: Reze and Moon gardener

## Use

Select **Reze · 3D** or **Moon gardener** in Settings → Skin. Both respond to chat/translation state, click, head hold, dragging, sleep and wake. The three interaction switches are saved with the existing config; older configs receive enabled defaults.

- Click: touch reaction and open chat (opening chat can be disabled).
- Hold the head for 450 ms: pet reaction without opening chat. Move more than 24 screen pixels to start dragging; body dragging starts after 4 pixels.
- Drag from the chest up: Reze is lifted (pendulum swing, flailing arms, kicking legs). Drag lower: she is slung over a shoulder, folded at the waist. Release: settle. Right click: existing menu. Enter: open chat.
- Global pointer position comes from the existing native 60 ms cursor poll. Coordinates are normalized to the 160×160 mascot surface. No coordinates are persisted.
- Reze turns her head within bounded angles, with damped tracking. Moon gardener looks in four directions. Neither follows the cursor by moving the desktop window.
- While idle, Reze picks a move every few seconds: sway, look around, hair tuck, stretch, the choker pin pull ("boom"), hug-knees sit or relaxed sit. Sleep is a dozing knee-hug. A poke plays a short voice clip (Settings → "Âm thanh khi chọc") and has a 1-in-4 chance to trigger the pin pull with a spark.
- Passive hover does not wake a sleeping companion. Click/hold wakes it. Reduced motion disables gaze and continuous movement, retaining static state poses.

## State flow

`MascotMachine.state` records system state; its timed `reaction` is separate. Touch, pet and drag temporarily overlay the system pose without cancelling thinking/talking. When the reaction expires, the current system pose reappears. Typing sets listening for five seconds, streaming sets talking, successful completion sets happy, failure sets error. Happy/error return to idle after 2.2 seconds. Thinking/talking are protected from inactivity sleep.

Hidden renderers stop drawing. Both renderers publish an alpha silhouette to the native hit mask so transparent pixels stay click-through. Renderer resources and event listeners are released when switching skins. A failed Reze load falls back to Moon gardener with a small error indicator.

## Reze assets and editing

- Original supplied file: `assets-src/reze/reze_de_chainsaw_man.glb` (unchanged; kept outside `public/` so it is not bundled).
- Runtime asset: `public/skins/reze/reze-companion.glb` (~7.4 MiB).
- Runtime camera/model/bone metadata: `src/mascot/reze-manifest.json`.
- Editable source generated locally: `artifacts/reze/reze-companion.blend`.
- Rebuild: run Blender in background with `--python scripts/prepare-reze.py` from the repository.

The preparation script normalizes height, welds split primitives, reduces mesh complexity, resizes textures to at most 1024, and adds an eleven-bone companion rig (head, torso, arms, thighs, shins) with 22 named animation actions. Poses are authored as world-space bone directions in `pose()`; `manifest.json` lists which actions are idle moves, one-shots and seated. It removes the duplicate inverted outline shell from the derived asset, because it renders incorrectly after skinning. The original download is preserved. The blend contains packed textures, a studio camera and an idle preview. Choose actions on RezeRig in Blender's Action Editor to refine weights and animation.

This is a lightweight companion rig: head, torso, arms and legs. It does **not** include independent eyeball tracking, blink/mouth morph targets, lip sync, fingers or a walking rig. Gaze is head motion; talking uses arm/head gestures. The current 3D loader supports the bundled Reze skin; third-party skin folders remain the existing sprite format.

Moon gardener's procedural sprite source is `scripts/create-skin.py`; run with Python + Pillow. It now has 22 frames including wave, pet hearts, pickup, settle, stretch and four-direction gaze. Existing third-party sprite skins can omit new reaction states and fall back to idle.

## Verification

Run `npm test`, `npm run build`, and `cargo check --manifest-path src-tauri/Cargo.toml`. With Vite on port 1420, run `npm run test:mascot` and `npm run test:companions`. The latter uses mocked IPC, verifies state/gesture interactions, skin switching and failure fallback, and captures app screenshots under `artifacts/reze`. These browser tests do not replace manual multi-monitor/DPI checks on the native desktop.

## Reze sounds

`public/skins/reze/sounds/*.mp3` are placeholder clips synthesized with Windows SAPI (Zira, pitch-shifted). Replace them with your own recordings using the same file names, or edit `sounds` in the manifest. Do not ship voice lines ripped from the anime.
