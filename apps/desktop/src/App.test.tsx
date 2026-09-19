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

describe('desktop app', () => {
  it('loads a definition and routes UI events through the bridge', async () => {
    const bridge: DesktopBridge = { prepareApp: vi.fn().mockResolvedValue({ token: 1, appId: 'hello', appName: 'Hello', requests: [] }), activateApp: vi.fn().mockResolvedValue(snapshot), dispatch: vi.fn().mockResolvedValue(snapshot) };
    render(<App bridge={bridge} />);
    fireEvent.change(screen.getByLabelText('AIX App Definition (JSON)'), { target: { value: '{}' } });
    fireEvent.click(screen.getByRole('button', { name: 'Review and load' }));
    await screen.findByRole('main', { name: 'Hello' });
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(bridge.dispatch).toHaveBeenCalledWith({ type: 'ui.click', target: 'refresh', payload: {} }));
    fireEvent.click(screen.getByRole('button', { name: 'Load another' }));
    expect(screen.getByLabelText('App loader')).toBeTruthy();
  });

  it('requires an explicit selection before sending a requested scope', async () => {
    const bridge: DesktopBridge = {
      prepareApp: vi.fn().mockResolvedValue({ token: 9, appId: 'weather', appName: 'Weather', requests: [{ capability: 'network.request', scopes: ['https://api.example.com/weather'] }] }),
      activateApp: vi.fn().mockResolvedValue(snapshot), dispatch: vi.fn()
    };
    render(<App bridge={bridge} />);
    fireEvent.change(screen.getByLabelText('AIX App Definition (JSON)'), { target: { value: '{}' } });
    fireEvent.click(screen.getByRole('button', { name: 'Review and load' }));
    await screen.findByRole('region', { name: 'Permission review' });
    fireEvent.click(screen.getByRole('button', { name: 'Load with selected permissions' }));
    await waitFor(() => expect(bridge.activateApp).toHaveBeenCalledWith(9, []));
  });
});
