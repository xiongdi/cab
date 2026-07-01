import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { createHash } from 'crypto';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');

// Helper to compute sha256 checksum of a file
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

  // Paths to release assets (can be overwritten or customized in CI)
  const dmgPath = join(
    ROOT,
    'src-tauri',
    'target',
    'release',
    'bundle',
    'dmg',
    `cab-desktop_${version}_universal.dmg`
  );
  const msiPath = join(
    ROOT,
    'src-tauri',
    'target',
    'release',
    'bundle',
    'msi',
    `cab-desktop_${version}_x64_en-US.msi`
  );

  // Precompiled binary tarball paths and source tarball path
  const tarMacosPath = join(ROOT, 'target', 'release', `cab-v${version}-macos-universal.tar.gz`);
  const tarLinuxX64Path = join(ROOT, 'target', 'release', `cab-v${version}-linux-x64.tar.gz`);
  const tarLinuxArm64Path = join(ROOT, 'target', 'release', `cab-v${version}-linux-arm64.tar.gz`);
  const tarSourcePath = join(ROOT, 'target', 'release', `v${version}.tar.gz`);

  const hashDmg = sha256(dmgPath);
  const hashMsi = sha256(msiPath);
  const hashMacos = sha256(tarMacosPath);
  const hashLinuxX64 = sha256(tarLinuxX64Path);
  const hashLinuxArm64 = sha256(tarLinuxArm64Path);
  const hashTar = sha256(tarSourcePath);

  const outDir = join(ROOT, 'dist', 'manifests');
  if (!existsSync(outDir)) {
    mkdirSync(outDir, { recursive: true });
  }
  // 1a. Homebrew Source Formula (For official Homebrew Core submission - builds from source)
  let sourceFormula = readFileSync(
    join(ROOT, 'packaging', 'brew', 'Formula', 'cab-source.rb'),
    'utf8'
  );
  sourceFormula = sourceFormula.replace(/v0\.5\.1/g, `v${version}`);
  sourceFormula = sourceFormula.replace(
    /"0000000000000000000000000000000000000000000000000000000000000000"/,
    `"${hashTar}"`
  );
  writeFileSync(join(outDir, 'cab-source.rb'), sourceFormula);
  console.log(`Generated: ${join(outDir, 'cab-source.rb')} (Homebrew Core style)`);

  // 1b. Homebrew Binary Formula (For Custom Tap - instant binary download)
  let binFormula = readFileSync(join(ROOT, 'packaging', 'brew', 'Formula', 'cab.rb'), 'utf8');
  binFormula = binFormula.replace(/version "0\.5\.1"/g, `version "${version}"`);
  binFormula = binFormula.replace(
    /"0000000000000000000000000000000000000000000000000000000000000000" # Replace with macOS universal binary checksum on release/,
    `"${hashMacos}"`
  );
  binFormula = binFormula.replace(
    /"0000000000000000000000000000000000000000000000000000000000000000" # Replace with Linux x64 binary checksum on release/,
    `"${hashLinuxX64}"`
  );
  binFormula = binFormula.replace(
    /"0000000000000000000000000000000000000000000000000000000000000000" # Replace with Linux arm64 binary checksum on release/,
    `"${hashLinuxArm64}"`
  );
  writeFileSync(join(outDir, 'cab-bin.rb'), binFormula);
  console.log(`Generated: ${join(outDir, 'cab-bin.rb')} (Custom Tap binary style)`);

  // 2. Homebrew Cask
  let cask = readFileSync(join(ROOT, 'packaging', 'brew', 'Casks', 'cab-desktop.rb'), 'utf8');
  cask = cask.replace(/"0.5.1"/g, `"${version}"`);
  cask = cask.replace(
    /"0000000000000000000000000000000000000000000000000000000000000000"/,
    `"${hashDmg}"`
  );
  writeFileSync(join(outDir, 'cab-desktop.rb'), cask);
  console.log(`Generated: ${join(outDir, 'cab-desktop.rb')}`);

  // 3. Winget Manifest
  let winget = readFileSync(join(ROOT, 'packaging', 'winget', 'xiongdi.cab.yaml'), 'utf8');
  winget = winget.replace(/PackageVersion: 0\.5\.1/g, `PackageVersion: ${version}`);
  winget = winget.replace(/v0\.5\.1/g, `v${version}`);
  winget = winget.replace(
    /cab-desktop_0\.5\.1_x64_en-US\.msi/g,
    `cab-desktop_${version}_x64_en-US.msi`
  );
  winget = winget.replace(
    /InstallerSha256: 0000000000000000000000000000000000000000000000000000000000000000/,
    `InstallerSha256: ${hashMsi}`
  );
  writeFileSync(join(outDir, 'xiongdi.cab.yaml'), winget);
  console.log(`Generated: ${join(outDir, 'xiongdi.cab.yaml')}`);

  console.log('All manifests generated successfully in dist/manifests/');
}

main();
