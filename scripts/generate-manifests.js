import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { createHash } from 'crypto';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');

function sha256(filePath) {
  if (!existsSync(filePath)) {
    console.warn(`Warning: File not found at ${filePath}. Using dummy hash.`);
    return '0000000000000000000000000000000000000000000000000000000000000000';
  }
  const fileBuffer = readFileSync(filePath);
  const hashSum = createHash('sha256');
  hashSum.update(fileBuffer);
  return hashSum.digest('hex');
}

function main() {
  const pkg = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));
  const version = pkg.version;
  console.log(`Generating manifests for version: v${version}`);

  const distCli = join(ROOT, 'dist', 'cli');
  const linuxX64 = join(distCli, 'cab-linux-x64.tar.gz');
  const linuxArm64 = join(distCli, 'cab-linux-arm64.tar.gz');
  const hashLinuxX64 = sha256(linuxX64);
  const hashLinuxArm64 = sha256(linuxArm64);

  const outDir = join(ROOT, 'dist', 'manifests');
  if (!existsSync(outDir)) {
    mkdirSync(outDir, { recursive: true });
  }

  let binFormula = readFileSync(join(ROOT, 'packaging', 'brew', 'Formula', 'cab.rb'), 'utf8');
  binFormula = binFormula.replace(/version "[^"]+"/, `version "${version}"`);
  // First placeholder = x64, second = arm64 (order in Formula).
  let replaced = 0;
  binFormula = binFormula.replace(
    /"0000000000000000000000000000000000000000000000000000000000000000"/g,
    () => {
      replaced += 1;
      return `"${replaced === 1 ? hashLinuxX64 : hashLinuxArm64}"`;
    }
  );
  writeFileSync(join(outDir, 'cab.rb'), binFormula);
  console.log(`Generated: ${join(outDir, 'cab.rb')}`);

  console.log('Manifests generated in dist/manifests/');
}

main();
