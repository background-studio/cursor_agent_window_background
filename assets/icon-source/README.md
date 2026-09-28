# Cursor Agent icon sources

The icon is assembled locally from separate layers, not generated as a single image.

1. `generation-request.json`: foreground-only prompt, same ice-white / cyan family as the other plugins.
2. `cursor-mark-generated.png`: original image-generation result.
3. `cursor-mark-transparent.png`: white field removed, mark kept.
4. `background-source.jpg`: unchanged copy of the supplied square upper-body artwork (963x963).
5. `background-40.png`: the source resized to the 896px tile, rounded corners, alpha scaled by **0.4**. Colours are not touched.
6. `frame.svg`: cyan rounded-square border.

The final 1024px RGBA master is `../icon.png`. Windows PNG and ICO go to `../../src-tauri/icons/`.

```powershell
npm install
npm run prepare-background
npm run compose
npm run verify
```

`npm run compose -- ../../../background-studio` also writes the host resource icon.
