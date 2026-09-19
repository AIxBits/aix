import { describe, expect, it, vi } from 'vitest';
import { createDesktopBridge } from './bridge.js';

describe('desktop bridge', () => {
  it('separates permission review, activation and typed dispatch', async () => {
    const invoke = vi.fn().mockResolvedValue({ appId: 'test' });
    const bridge = createDesktopBridge(invoke);
    await bridge.prepareApp('{"specVersion":"0.1.0"}');
    await bridge.activateApp(7, [{ capability: 'network.request', scopes: ['https://example.com'] }]);
    await bridge.dispatch({ type: 'ui.click', target: 'refresh', payload: {} });
    await bridge.listProviderProfiles();
    await bridge.saveProviderProfile({ id: 'local', name: 'Local', kind: 'local_open_ai_compatible', baseUrl: 'http://localhost:11434/v1', model: 'test' });
    await bridge.deleteProviderProfile('local');
    await bridge.generateApp({ profileId: 'local', requirement: 'Build an app' });
    await bridge.saveGeneratedDefinition('{}', 'yaml');
    expect(invoke).toHaveBeenNthCalledWith(1, 'prepare_app', { source: '{"specVersion":"0.1.0"}' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'activate_app', { token: 7, approvals: [{ capability: 'network.request', scopes: ['https://example.com'] }] });
    expect(invoke).toHaveBeenNthCalledWith(3, 'dispatch_event', { event: { type: 'ui.click', target: 'refresh', payload: {} } });
    expect(invoke).toHaveBeenNthCalledWith(4, 'list_provider_profiles');
    expect(invoke).toHaveBeenNthCalledWith(6, 'delete_provider_profile', { profileId: 'local' });
    expect(invoke).toHaveBeenNthCalledWith(8, 'save_generated_definition', { source: '{}', format: 'yaml' });
    expect(Object.keys(bridge)).toHaveLength(8);
  });
});
