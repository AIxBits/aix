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
    const bridge: DesktopBridge = { loadApp: vi.fn().mockResolvedValue(snapshot), dispatch: vi.fn().mockResolvedValue(snapshot) };
    render(<App bridge={bridge} />);
    fireEvent.change(screen.getByLabelText('AIX App Definition (JSON)'), { target: { value: '{}' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load app' }));
    await screen.findByRole('main', { name: 'Hello' });
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(bridge.dispatch).toHaveBeenCalledWith({ type: 'ui.click', target: 'refresh', payload: {} }));
    fireEvent.click(screen.getByRole('button', { name: 'Load another' }));
    expect(screen.getByLabelText('App loader')).toBeTruthy();
  });
});
