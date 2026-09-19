import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { App } from './App.js';
import type { DesktopBridge, DesktopSnapshot } from './bridge.js';

const snapshot: DesktopSnapshot = {
  appId: 'hello', appName: 'Hello', state: { city: 'Shanghai' }, resources: {}, timers: [],
  ui: { id: 'page', type: 'page', children: [
    { id: 'city', type: 'input', props: { value: { $state: '/city' } } },
    { id: 'refresh', type: 'button', props: { label: 'Refresh' } }
  ] }
};

function mockBridge(overrides: Partial<DesktopBridge> = {}): DesktopBridge {
  return {
    prepareApp: vi.fn(), activateApp: vi.fn(), dispatch: vi.fn(),
    listProviderProfiles: vi.fn().mockResolvedValue([]), saveProviderProfile: vi.fn(),
    deleteProviderProfile: vi.fn(), generateApp: vi.fn(), saveGeneratedDefinition: vi.fn(),
    ...overrides
  };
}

describe('desktop app', () => {
  it('loads a definition and routes UI events through the bridge', async () => {
    const bridge = mockBridge({ prepareApp: vi.fn().mockResolvedValue({ token: 1, appId: 'hello', appName: 'Hello', requests: [] }), activateApp: vi.fn().mockResolvedValue(snapshot), dispatch: vi.fn().mockResolvedValue(snapshot) });
    render(<App bridge={bridge} />);
    fireEvent.click(screen.getByRole('button', { name: 'Load JSON' }));
    fireEvent.change(screen.getByLabelText('AIX App Definition (JSON)'), { target: { value: '{}' } });
    fireEvent.click(screen.getByRole('button', { name: 'Review and load' }));
    await screen.findByRole('main', { name: 'Hello' });
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(bridge.dispatch).toHaveBeenCalledWith({ type: 'ui.click', target: 'refresh', payload: {} }));
    fireEvent.click(screen.getByRole('button', { name: 'Home' }));
    expect(screen.getByRole('navigation', { name: 'App source' })).toBeTruthy();
  });

  it('requires an explicit selection before sending a requested scope', async () => {
    const bridge = mockBridge({
      prepareApp: vi.fn().mockResolvedValue({ token: 9, appId: 'weather', appName: 'Weather', requests: [{ capability: 'network.request', scopes: ['https://api.example.com/weather'] }] }),
      activateApp: vi.fn().mockResolvedValue(snapshot), dispatch: vi.fn()
    });
    render(<App bridge={bridge} />);
    fireEvent.click(screen.getByRole('button', { name: 'Load JSON' }));
    fireEvent.change(screen.getByLabelText('AIX App Definition (JSON)'), { target: { value: '{}' } });
    fireEvent.click(screen.getByRole('button', { name: 'Review and load' }));
    await screen.findByRole('region', { name: 'Permission review' });
    fireEvent.click(screen.getByRole('button', { name: 'Load with selected permissions' }));
    await waitFor(() => expect(bridge.activateApp).toHaveBeenCalledWith(9, []));
  });

  it('generates a validated definition and offers review and export', async () => {
    const generated = { appId: 'hello', appName: 'Hello', source: '{}', attempts: 1, permissions: [], changes: [{ path: '/', kind: 'added' as const }] };
    const bridge = mockBridge({
      listProviderProfiles: vi.fn().mockResolvedValue([{ id: 'local', name: 'Local', kind: 'local_open_ai_compatible', baseUrl: 'http://localhost:11434/v1', model: 'test', hasApiKey: false }]),
      generateApp: vi.fn().mockResolvedValue(generated),
      saveGeneratedDefinition: vi.fn().mockResolvedValue({ path: '/tmp/hello.aix.json' })
    });
    render(<App bridge={bridge} />);
    await screen.findByRole('option', { name: 'Local · test' });
    fireEvent.change(screen.getByLabelText('Describe the app you want'), { target: { value: 'Create hello' } });
    fireEvent.click(screen.getByRole('button', { name: 'Generate definition' }));
    await screen.findByRole('region', { name: 'Generated definition review' });
    fireEvent.click(screen.getByRole('button', { name: 'Save JSON' }));
    await screen.findByText(/Saved to/);
    expect(bridge.generateApp).toHaveBeenCalledWith({ profileId: 'local', requirement: 'Create hello' });
    expect(bridge.saveGeneratedDefinition).toHaveBeenCalledWith('{}', 'json');
  });
});
