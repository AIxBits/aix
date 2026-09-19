/** App validation boundary. Parsing never evaluates application code. */
import { readFileSync } from 'node:fs';
import { Ajv2020 } from 'ajv/dist/2020.js';
import formats from 'ajv-formats';
import { parseDocument } from 'yaml';

const ajv = new Ajv2020({ allErrors: true, strict: true });
formats.default(ajv);
const check = ajv.compile(JSON.parse(readFileSync(new URL('../app.schema.json', import.meta.url), 'utf8')));
export interface ValidationResult { valid: boolean; errors: string[] }

/** Validate JSON structure plus IDs, event targets and acyclic workflow edges. */
export function validateApp(value: unknown): ValidationResult {
  if (!check(value)) return { valid: false, errors: (check.errors ?? []).map(e => `${e.instancePath || '/'} ${e.message}`) };
  const app = value as any;
  const errors: string[] = [];
  const ids = new Set<string>();
  function visitUi(node: any) {
    if (ids.has(node.id)) errors.push(`Duplicate UI id: ${node.id}`);
    ids.add(node.id);
    for (const child of node.children ?? []) visitUi(child);
  }
  visitUi(app.ui);
  const workflows = new Set<string>();
  for (const flow of app.workflows) {
    if (workflows.has(flow.id)) errors.push(`Duplicate workflow id: ${flow.id}`);
    workflows.add(flow.id);
    if (flow.on.type.startsWith('ui.') && !ids.has(flow.on.target)) errors.push(`Unknown event target: ${flow.on.target}`);
    if (flow.on.type === 'timer' && !flow.on.intervalMs) errors.push(`Timer requires intervalMs: ${flow.id}`);
    const steps = new Map<string, any>();
    for (const step of flow.steps) {
      if (steps.has(step.id)) errors.push(`Duplicate step: ${step.id}`);
      steps.set(step.id, step);
    }
    if (!steps.has(flow.entry)) errors.push(`Unknown entry: ${flow.entry}`);
    const visiting = new Set<string>(), done = new Set<string>();
    function walk(id: string) {
      if (!steps.has(id)) { errors.push(`Unknown step: ${id}`); return; }
      if (visiting.has(id)) { errors.push(`Workflow cycle: ${id}`); return; }
      if (done.has(id)) return;
      visiting.add(id);
      for (const edge of steps.get(id).next ?? []) walk(typeof edge === 'string' ? edge : edge.step);
      visiting.delete(id); done.add(id);
    }
    for (const id of steps.keys()) walk(id);
  }
  const connectorIds = new Set<string>(), operationIds = new Set<string>();
  for (const connector of app.connectors) {
    if (connectorIds.has(connector.id)) errors.push(`Duplicate connector: ${connector.id}`);
    connectorIds.add(connector.id);
    try {
      const base = new URL(connector.baseUrl);
      if (base.username || base.password || base.search || base.hash) errors.push(`Invalid connector baseUrl: ${connector.baseUrl}`);
    } catch {
      errors.push(`Invalid connector baseUrl: ${connector.baseUrl}`);
    }
    for (const operation of connector.operations) {
      if (operationIds.has(operation.id)) errors.push(`Duplicate connector operation: ${operation.id}`);
      operationIds.add(operation.id);
    }
  }
  return { valid: errors.length === 0, errors };
}

/** Parse JSON or YAML with bounded input and disabled YAML alias expansion. */
export function parseApp(source: string): unknown {
  if (Buffer.byteLength(source, 'utf8') > 1_048_576) throw new Error('App definition exceeds 1 MiB');
  const doc = parseDocument(source, { uniqueKeys: true });
  if (doc.errors.length) throw new Error(doc.errors.map(e => e.message).join('; '));
  return doc.toJS({ maxAliasCount: 0 });
}
