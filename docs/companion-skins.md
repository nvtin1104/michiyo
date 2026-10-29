# Companion skins

Moon gardener is the built-in sprite mascot. Select it or a custom sprite skin in Settings → Skin. It supports chat state, cursor tracking, touch, dragging and reduced motion.

Custom skins use a manifest and sprite sheet in the app data `skins/` directory. See the skin instructions in the README. Invalid skins fall back to the built-in mascot.

Run `npm test`, `npm run build`, and, with Vite on port 1420, `npm run test:mascot` and `npm run test:companions`. Browser tests use mocked IPC and do not replace native multi-monitor and DPI checks.
