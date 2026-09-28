import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const assetDirectory = path.dirname(fileURLToPath(import.meta.url));
const pluginDirectory = path.resolve(assetDirectory, '../..');

async function readPixels(relativePath) {
  return sharp(path.resolve(assetDirectory, relativePath))
    .ensureAlpha().raw().toBuffer({ resolveWithObject: true });
}

function readAlpha(image, column, row) {
  return image.data[(row * image.info.width + column) * 4 + 3];
}

const background = await readPixels('background-40.png');
const foreground = await readPixels('cursor-mark-transparent.png');
const composite = await readPixels('../icon.png');
for (const image of [background, foreground, composite]) {
  assert.equal(image.info.width, 1024);
  assert.equal(image.info.height, 1024);
  assert.equal(readAlpha(image, 0, 0), 0, 'Outside corners must be transparent.');
}
let maximumBackgroundAlpha = 0;
let opaqueMark = 0;
for (let alphaOffset = 3; alphaOffset < background.data.length; alphaOffset += 4) {
  maximumBackgroundAlpha = Math.max(maximumBackgroundAlpha, background.data[alphaOffset]);
}
for (let alphaOffset = 3; alphaOffset < foreground.data.length; alphaOffset += 4) {
  opaqueMark = Math.max(opaqueMark, foreground.data[alphaOffset]);
}
assert.equal(maximumBackgroundAlpha, 102, 'Photo opacity must be exactly 40%.');
assert.equal(opaqueMark, 255, 'The cursor mark must have an opaque stroke.');
assert.equal(readAlpha(composite, 8, 8), 0, 'The icon corner outside the tile must stay transparent.');

const windowsIcon = await readFile(path.join(pluginDirectory, 'src-tauri/icons/icon.ico'));
const expectedDimensions = [16, 24, 32, 48, 64, 128, 256];
assert.equal(windowsIcon.readUInt16LE(2), 1);
assert.equal(windowsIcon.readUInt16LE(4), expectedDimensions.length);
for (const [imageIndex, dimension] of expectedDimensions.entries()) {
  const entryOffset = 6 + imageIndex * 16;
  const imageLength = windowsIcon.readUInt32LE(entryOffset + 8);
  const imageOffset = windowsIcon.readUInt32LE(entryOffset + 12);
  const metadata = await sharp(windowsIcon.subarray(imageOffset, imageOffset + imageLength)).metadata();
  assert.equal(metadata.width, dimension);
  assert.equal(metadata.height, dimension);
  assert.equal(metadata.hasAlpha, true);
}

console.log('PASS: 40% upper-body photo, transparent corners, opaque cursor mark, seven ICO sizes.');
