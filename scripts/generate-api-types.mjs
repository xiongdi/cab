#!/usr/bin/env node
/**
 * Generate src/lib/api-types.ts from spec/src/content/docs/modules/openapi.yaml.
 * This is the single source of truth for API types — do not edit api-types.ts manually.
 * Run: npm run generate-types
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import * as yaml from 'js-yaml';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const openapiPath = path.join(root, 'spec/src/content/docs/modules/openapi.yaml');
const outPath = path.join(root, 'src/lib/api-types.ts');

const spec = yaml.load(readFileSync(openapiPath, 'utf8'));
const schemas = spec.components?.schemas ?? {};

function tsType(prop) {
  if (prop.$ref) {
    return prop.$ref.replace('#/components/schemas/', '');
  }
  if (prop.allOf) {
    return prop.allOf.map(tsType).join(' & ');
  }
  if (prop.oneOf) {
    return prop.oneOf.map(tsType).join(' | ');
  }
  const nullable = prop.nullable === true;
  let base;
  switch (prop.type) {
    case 'string':
      base = prop.enum ? prop.enum.map((v) => `'${v}'`).join(' | ') : 'string';
      break;
    case 'integer':
    case 'number':
      base = 'number';
      break;
    case 'boolean':
      base = 'boolean';
      break;
    case 'array':
      base = `Array<${tsType(prop.items ?? {})}>`;
      break;
    case 'object':
      if (prop.additionalProperties) {
        const ap = prop.additionalProperties;
        if (ap.$ref) {
          base = `Record<string, ${tsType(ap)}>`;
        } else if (ap === true || ap.type === undefined) {
          base = 'Record<string, unknown>';
        } else {
          base = `Record<string, ${tsType(ap)}>`;
        }
      } else {
        base = 'Record<string, unknown>';
      }
      break;
    default:
      base = 'unknown';
  }
  return nullable ? `${base} | null` : base;
}

function collectProps(schema, schemas) {
  const props = {};
  if (schema.$ref) {
    const refName = schema.$ref.replace('#/components/schemas/', '');
    const refSchema = schemas[refName];
    if (refSchema) {
      Object.assign(props, collectProps(refSchema, schemas));
    }
    return props;
  }
  if (schema.allOf) {
    for (const part of schema.allOf) {
      Object.assign(props, collectProps(part, schemas));
    }
    return props;
  }
  if (schema.properties) {
    Object.assign(props, schema.properties);
  }
  return props;
}

function collectRequired(schema, schemas) {
  const required = new Set();
  if (schema.$ref) {
    const refName = schema.$ref.replace('#/components/schemas/', '');
    const refSchema = schemas[refName];
    if (refSchema) {
      for (const r of collectRequired(refSchema, schemas)) required.add(r);
    }
    return required;
  }
  if (schema.allOf) {
    for (const part of schema.allOf) {
      for (const r of collectRequired(part, schemas)) required.add(r);
    }
    return required;
  }
  for (const r of schema.required ?? []) required.add(r);
  return required;
}

function genInterface(name, schema) {
  const required = collectRequired(schema, schemas);
  const props = collectProps(schema, schemas);
  const lines = [];
  for (const [key, prop] of Object.entries(props)) {
    const optional = !required.has(key);
    const type = tsType(prop);
    const desc = prop.description ? `  /** ${prop.description} */\n` : '';
    lines.push(`${desc}  ${key}${optional ? '?' : ''}: ${type};`);
  }
  return `export interface ${name} {\n${lines.join('\n')}\n}`;
}

const interfaces = [];
for (const [name, schema] of Object.entries(schemas)) {
  if (schema.type === 'object' || schema.properties || schema.allOf) {
    interfaces.push(genInterface(name, schema));
  } else if (schema.type === 'array') {
    interfaces.push(`export type ${name} = Array<${tsType(schema.items ?? {})}>;`);
  } else {
    interfaces.push(`export type ${name} = ${tsType(schema)};`);
  }
}

const header = `// ═══════════════════════════════════════════════════════════════
// GENERATED CODE — DO NOT EDIT MANUALLY
// Source: spec/src/content/docs/modules/openapi.yaml
// Run \`npm run generate-types\` to regenerate
// ═══════════════════════════════════════════════════════════════

`;

const body = interfaces.join('\n\n') + '\n';
writeFileSync(outPath, header + body);
console.log(`Generated ${interfaces.length} types → ${path.relative(root, outPath)}`);
