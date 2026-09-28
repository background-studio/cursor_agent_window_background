import { copyFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const assetDirectory = path.dirname(fileURLToPath(import.meta.url));
const sourcePath = path.join(assetDirectory, 'background-source.jpg');
const suppliedSourcePath = process.argv[2];

if (suppliedSourcePath && path.resolve(suppliedSourcePath) !== sourcePath) {
  await copyFile(suppliedSourcePath, sourcePath);
}

const canvasSize = 1024;
const tileInset = 64;
const tileSize = canvasSize - tileInset * 2;
const cornerRadius = 220;
const backgroundOpacity = 0.4;

const metadata = await sharp(sourcePath).metadata();
if (metadata.width !== metadata.height) {
  throw new Error(`The supplied artwork must already be square, got ${metadata.width}x${metadata.height}.`);
}

// Only the alpha channel changes; colour, brightness and crop stay as supplied.
const roundedMask = Buffer.from(
  `<svg width="${tileSize}" height="${tileSize}">
    <rect width="${tileSize}" height="${tileSize}" rx="${cornerRadius}" fill="white"/>
  </svg>`,
);

const resizedTile = await sharp(sourcePath)
  .resize(tileSize, tileSize)
  .ensureAlpha()
  .png()
  .toBuffer();

const { data: backgroundPixels, info: backgroundInfo } = await sharp(resizedTile)
  .composite([{ input: roundedMask, blend: 'dest-in' }])
  .raw()
  .toBuffer({ resolveWithObject: true });

for (let alphaOffset = 3; alphaOffset < backgroundPixels.length; alphaOffset += 4) {
  backgroundPixels[alphaOffset] = Math.round(
    backgroundPixels[alphaOffset] * backgroundOpacity,
  );
}

await sharp(backgroundPixels, { raw: backgroundInfo })
  .extend({
    top: tileInset,
    bottom: tileInset,
    left: tileInset,
    right: tileInset,
    background: { r: 0, g: 0, b: 0, alpha: 0 },
  })
  .png()
  .toFile(path.join(assetDirectory, 'background-40.png'));

console.log('Prepared background-40.png: 1024x1024, source colours unchanged, maximum alpha 102/255 (40%).');
