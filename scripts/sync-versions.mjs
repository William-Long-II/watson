import { readFileSync, writeFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const rootDir = join(__dirname, '..');

// Get version from package.json
const packageJson = JSON.parse(readFileSync(join(rootDir, 'package.json'), 'utf8'));
const version = packageJson.version;

console.log(`Syncing version ${version} across all files...`);

// Update Cargo.toml
const cargoPath = join(rootDir, 'src-tauri', 'Cargo.toml');
let cargoContent = readFileSync(cargoPath, 'utf8');
cargoContent = cargoContent.replace(/^version = ".*"/m, `version = "${version}"`);
writeFileSync(cargoPath, cargoContent);
console.log(`  Updated src-tauri/Cargo.toml`);

// Update tauri.conf.json
const tauriConfPath = join(rootDir, 'src-tauri', 'tauri.conf.json');
const tauriConf = JSON.parse(readFileSync(tauriConfPath, 'utf8'));
tauriConf.version = version;
writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');
console.log(`  Updated src-tauri/tauri.conf.json`);

console.log(`\nVersion ${version} synced to all files!`);
