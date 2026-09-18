import { invoke } from '@tauri-apps/api/core';
import type { RenderSnapshot, TimerSubscription, UiEvent } from '@aix/ui';

export interface DesktopSnapshot extends RenderSnapshot { timers: TimerSubscription[] }
export type RuntimeDispatchEvent = UiEvent | { type: 'timer'; workflowId: string; payload: null };
export type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export interface DesktopBridge {
  loadApp(source: string): Promise<DesktopSnapshot>;
  dispatch(event: RuntimeDispatchEvent): Promise<DesktopSnapshot>;
}

/** Typed IPC facade; React never invokes Operations or native capabilities directly. */
export function createDesktopBridge(call: Invoke = invoke): DesktopBridge {
  return {
    loadApp: source => call<DesktopSnapshot>('load_app', { source }),
    dispatch: event => call<DesktopSnapshot>('dispatch_event', { event })
  };
}
