#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';

const target = process.argv[2] ?? 'all';

function run(command, args) {
  const result = spawnSync(resolveCommand(command), args, { stdio: 'inherit' });
  if (result.error) {
    console.error(result.error.message);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function hasCommand(command, args) {
  const result = spawnSync(resolveCommand(command), args, { stdio: 'ignore' });
  return result.status === 0;
}

function resolveCommand(command) {
  if (process.platform === 'win32' && command === 'npm') {
    return 'npm.cmd';
  }
  return command;
}

function runBackend() {
  if (!hasCommand('cargo', ['llvm-cov', '--version'])) {
    console.error('cargo-llvm-cov is required for Rust coverage.');
    console.error('Install it with: cargo install cargo-llvm-cov');
    process.exit(1);
  }

  mkdirSync('target/coverage', { recursive: true });
  run('cargo', ['llvm-cov', 'clean', '--workspace']);
  run('cargo', [
    'llvm-cov',
    '--workspace',
    '--all-targets',
    '--lcov',
    '--output-path',
    'target/coverage/lcov.info',
  ]);
  run('cargo', ['llvm-cov', 'report']);
}

function runFrontend() {
  run('npm', ['run', 'coverage:frontend']);
}

switch (target) {
  case 'backend':
    runBackend();
    break;
  case 'frontend':
    runFrontend();
    break;
  case 'all':
    runFrontend();
    runBackend();
    break;
  default:
    console.error('usage: node scripts/run-coverage.mjs [all|backend|frontend]');
    process.exit(2);
}
