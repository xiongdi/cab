#!/usr/bin/env node
/**
 * Sync selected OpenAPI schemas into src/lib/types.ts markers.
 * Full codegen can replace this once utoipa export lands in cab-api.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const typesPath = path.join(root, 'src/lib/types.ts');
const openapiPath = path.join(root, 'spec/src/content/docs/modules/openapi.yaml');

const source = readFileSync(typesPath, 'utf8');
if (!source.includes('RouteExplainRequest')) {
  console.error('RouteExplain types missing from src/lib/types.ts — add them manually first.');
  process.exit(1);
}

const openapi = readFileSync(openapiPath, 'utf8');
if (!openapi.includes('/api/routing/explain')) {
  console.error('openapi.yaml is missing /api/routing/explain');
  process.exit(1);
}

const stamp = `// Generated from ${path.relative(root, openapiPath)} on ${new Date().toISOString()}\n`;
const marker = '// OPENAPI_SYNC_MARKER';
let next = source;
if (source.includes(marker)) {
  next = source.replace(/\/\/ OPENAPI_SYNC_MARKER[\s\S]*?\/\/ END_OPENAPI_SYNC_MARKER\n?/, '');
}
next = next.trimEnd() + `\n\n${marker}\n${stamp}// END_OPENAPI_SYNC_MARKER\n`;
writeFileSync(typesPath, next);
console.log(`Synced OpenAPI marker in ${path.relative(root, typesPath)}`);
