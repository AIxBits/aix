const MAX_UI_BINDING_DEPTH = 64;

/** Resolve renderer-only `$state` bindings without expressions or evaluation. */
export function resolveUiValue(value: unknown, state: Record<string, unknown>): unknown {
  return resolve(value, state, 0);
}

function resolve(value: unknown, state: Record<string, unknown>, depth: number): unknown {
  if (depth > MAX_UI_BINDING_DEPTH) throw new Error('UI binding depth exceeded');
  if (Array.isArray(value)) return value.map(item => resolve(item, state, depth + 1));
  if (!isRecord(value)) return value;
  const keys = Object.keys(value);
  if (keys.some(key => key.startsWith('$'))) {
    if (keys.length !== 1 || typeof value.$state !== 'string') {
      throw new Error('UI binding must contain exactly one string `$state` field');
    }
    return readState(state, value.$state);
  }
  return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, resolve(item, state, depth + 1)]));
}

function readState(state: Record<string, unknown>, path: string): unknown {
  if (path === '') return state;
  if (!path.startsWith('/')) return state[path];
  let current: unknown = state;
  for (const encodedToken of path.slice(1).split('/')) {
    const token = decodeToken(encodedToken);
    if (Array.isArray(current)) {
      if (!/^\d+$/.test(token)) return undefined;
      current = current[Number(token)];
    } else if (isRecord(current)) {
      current = current[token];
    } else {
      return undefined;
    }
  }
  return current;
}

function decodeToken(token: string): string {
  if (/~(?:[^01]|$)/.test(token)) throw new Error('Invalid JSON Pointer escape');
  return token.replace(/~1/g, '/').replace(/~0/g, '~');
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
