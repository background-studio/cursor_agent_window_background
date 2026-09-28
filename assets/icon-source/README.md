# Cursor Agent icon sources

The icon is assembled locally from separate layers, not generated as a single image.

1. `generation-request.json`: foreground-only prompt, same ice-white / cyan family as the other plugins.
2. `cursor-mark-generated.png`: original image-generation result.
3. `cursor-mark-transparent.png`: white field removed, mark kept.
4. `background-source.webp`: unchanged copy of the supplied artwork.
5. `background-40.png`: upper-body square (`left 780, top 0, 1208x1208` of the 2160x1208 source), rounded corners, alpha scaled by **0.4**.
6. `frame.svg`: cyan rounded-square border.

The final 1024px RGBA master is `../icon.png`. Windows PNG and ICO go to `../../src-tauri/icons/`.

```powershell
npm install
npm run prepare-background
npm run compose
npm run verify
```

`npm run compose -- ../../../background-studio` also writes the host resource icon.
