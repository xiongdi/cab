import { spawnSync } from 'child_process';
import { existsSync, mkdirSync, copyFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..');
const IS_WINDOWS = process.platform === 'win32';

function run(cmd, args) {
  console.log(`Running: ${cmd} ${args.join(' ')}`);
  const res = spawnSync(cmd, args, { stdio: 'inherit', shell: true, cwd: ROOT });
  if (res.status !== 0) {
    console.error(`Command failed: ${cmd} ${args.join(' ')} (exit code ${res.status})`);
    process.exit(res.status || 1);
  }
}

// 1. Build the Svelte frontend assets
run('npm', ['run', 'build']);

// 2. Build the Rust release binaries
const targetFlag = process.env.CAB_CARGO_TARGET ? ['--target', process.env.CAB_CARGO_TARGET] : [];
run('cargo', ['build', '--release', '-p', 'cab', '-p', 'cab-srv', ...targetFlag]);

// 3. Create the unified resources bin directory
const resourcesBinDir = join(ROOT, 'resources', 'bin');
if (!existsSync(resourcesBinDir)) {
  mkdirSync(resourcesBinDir, { recursive: true });
}

// 4. Copy the compiled binaries to resources/bin
const ext = IS_WINDOWS ? '.exe' : '';
const cabSrc = join(ROOT, 'target', 'release', `cab-cli${ext}`);
const cabSrvSrc = join(ROOT, 'target', 'release', `cab-srv${ext}`);
const cabDst = join(resourcesBinDir, `cab-cli${ext}`);
const cabSrvDst = join(resourcesBinDir, `cab-srv${ext}`);

console.log(`Copying ${cabSrc} to ${cabDst}`);
copyFileSync(cabSrc, cabDst);

console.log(`Copying ${cabSrvSrc} to ${cabSrvDst}`);
copyFileSync(cabSrvSrc, cabSrvDst);

console.log('Pre-build steps completed successfully.');
