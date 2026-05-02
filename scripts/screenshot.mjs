// scripts/screenshot.mjs — quick capture helper for design iteration.
//
// Spins up a headless Brave, navigates to the running Vite dev server
// (or any URL), takes a screenshot, exits. Also flips the .dark class
// when --dark is passed so we can see the aero-glass treatment without
// hand-toggling Settings every time.
//
// Usage:
//   node scripts/screenshot.mjs --out path/to/out.png
//   node scripts/screenshot.mjs --out dark.png --dark
//   node scripts/screenshot.mjs --out wide.png --width 800 --height 600
//
// Brave path is hardcoded for the maintainer's box; override via the
// PUPPETEER_EXECUTABLE_PATH env var if it moves.

import puppeteer from 'puppeteer-core';

const BRAVE_PATH =
  process.env.PUPPETEER_EXECUTABLE_PATH ||
  'C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe';

const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce((acc, arg, i, all) => {
      if (arg.startsWith('--')) {
        const key = arg.slice(2);
        const next = all[i + 1];
        const isFlag = !next || next.startsWith('--');
        acc.push([key, isFlag ? true : next]);
      }
      return acc;
    }, []),
);

const url = args.url || 'http://localhost:1420';
const out = args.out || 'screenshot.png';
const width = Number(args.width) || 600;
const height = Number(args.height) || 400;
const dark = Boolean(args.dark);

const browser = await puppeteer.launch({
  executablePath: BRAVE_PATH,
  headless: 'new',
  args: ['--no-sandbox', '--disable-blink-features=AutomationControlled'],
});

try {
  const page = await browser.newPage();
  await page.setViewport({ width, height, deviceScaleFactor: 2 });
  await page.goto(url, { waitUntil: 'networkidle0', timeout: 15000 });
  if (dark) {
    await page.evaluate(() => {
      document.documentElement.classList.add('dark');
    });
    // Give CSS a tick to apply.
    await new Promise((r) => setTimeout(r, 100));
  }
  await page.screenshot({ path: out, fullPage: false });
  console.log(`saved → ${out}`);
} finally {
  await browser.close();
}
