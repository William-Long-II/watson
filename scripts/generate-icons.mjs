import sharp from 'sharp';
import { readFileSync, writeFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const iconsDir = join(__dirname, '..', 'src-tauri', 'icons');

const svgContent = readFileSync(join(iconsDir, 'icon.svg'));

const sizes = [
  { name: '32x32.png', size: 32 },
  { name: '128x128.png', size: 128 },
  { name: '128x128@2x.png', size: 256 },
  { name: 'icon.png', size: 512 },
];

async function generateIcons() {
  for (const { name, size } of sizes) {
    await sharp(svgContent)
      .resize(size, size)
      .png()
      .toFile(join(iconsDir, name));
    console.log(`Generated ${name}`);
  }

  // Also generate ICO for Windows (just copy the 256px as base)
  await sharp(svgContent)
    .resize(256, 256)
    .png()
    .toFile(join(iconsDir, 'icon.ico'));
  console.log('Generated icon.ico (as PNG, may need conversion for true ICO)');
}

generateIcons().catch(console.error);
