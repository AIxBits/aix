import { describe, expect, it, vi } from 'vitest';
import { createDesktopBridge } from './bridge.js';

describe('desktop bridge', () => {
  it('separates permission review, activation and typed dispatch', async () => {
    const invoke = vi.fn().mockResolvedValue({ appId: 'test' });
    const bridge = createDesktopBridge(invoke);
    await bridge.prepareApp('{"specVersion":"0.1.0"}');
    await bridge.activateApp(7, [{ capability: 'network.request', scopes: ['https://example.com'] }]);
    await bridge.dispatch({ type: 'ui.click', target: 'refresh', payload: {} });
    expect(invoke).toHaveBeenNthCalledWith(1, 'prepare_app', { source: '{"specVersion":"0.1.0"}' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'activate_app', { token: 7, approvals: [{ capability: 'network.request', scopes: ['https://example.com'] }] });
    expect(invoke).toHaveBeenNthCalledWith(3, 'dispatch_event', { event: { type: 'ui.click', target: 'refresh', payload: {} } });
    expect(Object.keys(bridge).sort()).toEqual(['activateApp', 'dispatch', 'prepareApp']);
  });
});
