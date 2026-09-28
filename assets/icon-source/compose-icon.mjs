import { copyFile, mkdir, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const assetDirectory = path.dirname(fileURLToPath(import.meta.url));
const pluginDirectory = path.resolve(assetDirectory, '../..');
const iconDirectory = path.join(pluginDirectory, 'src-tauri/icons');
const hostDirectory = process.argv[2] ? path.resolve(process.argv[2]) : null;
const canvasSize = 1024;

const { data: markPixels, info: markInfo } = await sharp(
  path.join(assetDirectory, 'cursor-mark-generated.png'),
).ensureAlpha().raw().toBuffer({ resolveWithObject: true });

if (markInfo.width !== canvasSize || markInfo.height !== canvasSize || markInfo.channels !== 4) {
  throw new Error('The generated foreground must be a 1024x1024 RGBA image.');
}

// The generator returned an opaque white field. Flood near-white pixels that
// touch the canvas edge so the cursor fill stays, and the outside becomes alpha.
const nearWhite = (offset) => {
  const red = markPixels[offset];
  const green = markPixels[offset + 1];
  const blue = markPixels[offset + 2];
  return red > 246 && green > 246 && blue > 246 && (Math.max(red, green, blue) - Math.min(red, green, blue)) < 14;
};
const seen = new Uint8Array(canvasSize * canvasSize);
const queue = [];
const push = (x, y) => {
  const index = y * canvasSize + x;
  if (seen[index]) return;
  if (!nearWhite(index * 4)) return;
  seen[index] = 1;
  queue.push(index);
};
for (let x = 0; x < canvasSize; x += 1) {
  push(x, 0);
  push(x, canvasSize - 1);
}
for (let y = 0; y < canvasSize; y += 1) {
  push(0, y);
  push(canvasSize - 1, y);
}
for (let cursor = 0; cursor < queue.length; cursor += 1) {
  const index = queue[cursor];
  const x = index % canvasSize;
  const y = (index - x) / canvasSize;
  if (x > 0) push(x - 1, y);
  if (x + 1 < canvasSize) push(x + 1, y);
  if (y > 0) push(x, y - 1);
  if (y + 1 < canvasSize) push(x, y + 1);
}
for (let index = 0; index < seen.length; index += 1) {
  if (!seen[index]) continue;
  const offset = index * 4;
  markPixels.fill(0, offset, offset + 4);
}

const foregroundPath = path.join(assetDirectory, 'cursor-mark-transparent.png');
await sharp(markPixels, { raw: markInfo }).png().toFile(foregroundPath);

const frameSource = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <rect x="74" y="74" width="876" height="876" rx="208" fill="none" stroke="#20414C" stroke-width="22" stroke-opacity="0.35"/>
  <rect x="74" y="74" width="876" height="876" rx="208" fill="none" stroke="#61D9EC" stroke-width="10" stroke-opacity="0.90"/>
  <rect x="80" y="80" width="864" height="864" rx="202" fill="none" stroke="#C9F4FC" stroke-width="1.5" stroke-opacity="0.40"/>
</svg>`;
await writeFile(path.join(assetDirectory, 'frame.svg'), `${frameSource}\n`);

const composedIcon = await sharp(path.join(assetDirectory, 'background-40.png'))
  .composite([
    { input: Buffer.from(frameSource) },
    { input: foregroundPath },
  ])
  .png()
  .toBuffer();
const masterPath = path.join(pluginDirectory, 'assets/icon.png');
await mkdir(path.dirname(masterPath), { recursive: true });
await writeFile(masterPath, composedIcon);

for (const [theme, background] of [['dark', '#161D27'], ['light', '#F2F5F9']]) {
  await sharp(composedIcon).flatten({ background }).resize(512, 512)
    .png().toFile(path.join(assetDirectory, `preview-${theme}.png`));
}

await mkdir(iconDirectory, { recursive: true });
await sharp(composedIcon).resize(256, 256).png().toFile(path.join(iconDirectory, 'icon.png'));

async function createWindowsIcon() {
  const dimensions = [16, 24, 32, 48, 64, 128, 256];
  const images = await Promise.all(dimensions.map((dimension) =>
    sharp(composedIcon).resize(dimension, dimension).png().toBuffer(),
  ));
  const directory = Buffer.alloc(6 + dimensions.length * 16);
  directory.writeUInt16LE(1, 2);
  directory.writeUInt16LE(dimensions.length, 4);
  let imageOffset = directory.length;
  for (const [imageIndex, dimension] of dimensions.entries()) {
    const entryOffset = 6 + imageIndex * 16;
    directory[entryOffset] = dimension === 256 ? 0 : dimension;
    directory[entryOffset + 1] = dimension === 256 ? 0 : dimension;
    directory.writeUInt16LE(1, entryOffset + 4);
    directory.writeUInt16LE(32, entryOffset + 6);
    directory.writeUInt32LE(images[imageIndex].length, entryOffset + 8);
    directory.writeUInt32LE(imageOffset, entryOffset + 12);
    imageOffset += images[imageIndex].length;
  }
  return Buffer.concat([directory, ...images]);
}

const windowsIcon = await createWindowsIcon();
await writeFile(path.join(iconDirectory, 'icon.ico'), windowsIcon);

if (hostDirectory) {
  const hostIcon = await sharp(composedIcon).resize(512, 512).png().toBuffer();
  const resourcePath = path.join(hostDirectory, 'src-tauri/resources/cursor-agent.png');
  await mkdir(path.dirname(resourcePath), { recursive: true });
  await writeFile(resourcePath, hostIcon);
  const publicPath = path.join(hostDirectory, 'public/plugins/cursor-agent.png');
  await mkdir(path.dirname(publicPath), { recursive: true });
  await copyFile(resourcePath, publicPath);
}

const exported = (await readdir(iconDirectory)).filter((name) => name.endsWith('.png') || name.endsWith('.ico'));
console.log(`Composed ${masterPath}: 40% upper-body photo, cyan frame, cursor mark.`);
console.log(`Exported ${exported.join(', ')}.`);
