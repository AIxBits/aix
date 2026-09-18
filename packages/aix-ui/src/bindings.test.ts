import { describe, expect, it } from 'vitest';
import { resolveUiValue } from './bindings.js';

describe('resolveUiValue', () => {
  it('resolves JSON Pointer and legacy top-level state bindings', () => {
    const state = { profile: { name: 'Ada' }, city: 'London', 'a/b': 3 };
    expect(resolveUiValue({ value: { $state: '/profile/name' } }, state)).toEqual({ value: 'Ada' });
    expect(resolveUiValue({ $state: 'city' }, state)).toBe('London');
    expect(resolveUiValue({ $state: '/a~1b' }, state)).toBe(3);
  });

  it('rejects ambiguous bindings and excessive nesting', () => {
    expect(() => resolveUiValue({ $state: '/city', extra: true }, { city: 'London' })).toThrow();
    let value: unknown = 'leaf';
    for (let index = 0; index < 66; index += 1) value = [value];
    expect(() => resolveUiValue(value, {})).toThrow('depth');
  });
});
