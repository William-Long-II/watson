import sharp from 'sharp';
import pngToIco from 'png-to-ico';
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
  // Generate PNG icons
  for (const { name, size } of sizes) {
    await sharp(svgContent)
      .resize(size, size)
      .png()
      .toFile(join(iconsDir, name));
    console.log(`Generated ${name}`);
  }

  // Generate proper Windows ICO file with multiple sizes
  const icoSizes = [16, 32, 48, 64, 128, 256];
  const pngBuffers = [];

  for (const size of icoSizes) {
    const buffer = await sharp(svgContent)
      .resize(size, size)
      .png()
      .toBuffer();
    pngBuffers.push(buffer);
  }

  const icoBuffer = await pngToIco(pngBuffers);
  writeFileSync(join(iconsDir, 'icon.ico'), icoBuffer);
  console.log('Generated icon.ico (proper Windows ICO with multiple sizes)');
}

generateIcons().catch(console.error);
