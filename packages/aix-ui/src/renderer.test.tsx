import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AixRenderer } from './renderer.js';
import type { RenderSnapshot } from './protocol.js';

function snapshot(): RenderSnapshot {
  return {
    appId: 'test', appName: 'Test App',
    state: { city: 'Shanghai', rows: [{ city: 'Paris', temperature: 20 }], alerts: ['Rain', 'Wind'] },
    resources: {
      title: { type: 'text', text: 'Weather' },
      image: { type: 'image', uri: 'data:image/png;base64,AA==' },
      audio: { type: 'audio', uri: 'data:audio/mpeg;base64,AA==' },
      video: { type: 'video', uri: 'data:video/mp4;base64,AA==' },
      blocked: { type: 'unavailable', reason: 'external media is disabled' }
    },
    ui: { id: 'page', type: 'page', children: [
      { id: 'group', type: 'container', props: { label: 'Summary' }, children: [
        { id: 'alerts', type: 'list', props: { items: { $state: '/alerts' } } }
      ] },
      { id: 'title', type: 'text', props: { resource: 'title' } },
      { id: 'city', type: 'input', props: { label: 'City', value: { $state: '/city' } } },
      { id: 'refresh', type: 'button', props: { label: 'Refresh' } },
      { id: 'table', type: 'table', props: { rows: { $state: '/rows' } } },
      { id: 'picture', type: 'image', props: { resource: 'image', alt: 'Weather icon' } },
      { id: 'sound', type: 'audio', props: { resource: 'audio' } },
      { id: 'clip', type: 'video', props: { resource: 'video' } },
      { id: 'blocked', type: 'video', props: { resource: 'blocked' } }
    ] }
  };
}

describe('AixRenderer', () => {
  it('renders state, structured data and host resource handles', () => {
    render(<AixRenderer snapshot={snapshot()} dispatch={() => undefined} />);
    expect(screen.getByText('Weather')).toBeTruthy();
    expect((screen.getByLabelText('City') as HTMLInputElement).value).toBe('Shanghai');
    expect(screen.getByText('Paris')).toBeTruthy();
    expect(screen.getByText('Rain')).toBeTruthy();
    expect(screen.getByRole('region', { name: 'Summary' })).toBeTruthy();
    expect(screen.getByAltText('Weather icon').getAttribute('src')).toBe('data:image/png;base64,AA==');
    expect(document.querySelector('audio')?.getAttribute('src')).toBe('data:audio/mpeg;base64,AA==');
    expect(document.querySelector('video[src="data:video/mp4;base64,AA=="]')).toBeTruthy();
    expect(screen.getByText('Resource unavailable')).toBeTruthy();
  });

  it('emits typed events without running business logic', () => {
    const dispatch = vi.fn();
    render(<AixRenderer snapshot={snapshot()} dispatch={dispatch} />);
    fireEvent.change(screen.getByLabelText('City'), { target: { value: 'Tokyo' } });
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    expect(dispatch).toHaveBeenNthCalledWith(1, { type: 'ui.change', target: 'city', payload: { value: 'Tokyo' } });
    expect(dispatch).toHaveBeenNthCalledWith(2, { type: 'ui.click', target: 'refresh', payload: {} });
  });
});
