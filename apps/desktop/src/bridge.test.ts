import { describe, expect, it, vi } from 'vitest';
import { createDesktopBridge } from './bridge.js';

describe('desktop bridge', () => {
  it('exposes only definition loading and typed event dispatch', async () => {
    const invoke = vi.fn().mockResolvedValue({ appId: 'test' });
    const bridge = createDesktopBridge(invoke);
    await bridge.loadApp('{"specVersion":"0.1.0"}');
    await bridge.dispatch({ type: 'ui.click', target: 'refresh', payload: {} });
    expect(invoke).toHaveBeenNthCalledWith(1, 'load_app', { source: '{"specVersion":"0.1.0"}' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'dispatch_event', { event: { type: 'ui.click', target: 'refresh', payload: {} } });
    expect(Object.keys(bridge).sort()).toEqual(['dispatch', 'loadApp']);
  });
});
