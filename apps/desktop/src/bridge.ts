import { invoke } from '@tauri-apps/api/core';
import type { RenderSnapshot, TimerSubscription, UiEvent } from '@aix/ui';

export interface DesktopSnapshot extends RenderSnapshot { timers: TimerSubscription[] }
export interface PermissionRequest { capability: string; scopes: string[] }
export interface PermissionPreview { token: number; appId: string; appName: string; requests: PermissionRequest[] }
export interface PermissionApproval { capability: string; scopes: string[] }
export type RuntimeDispatchEvent = UiEvent | { type: 'timer'; workflowId: string; payload: null };
export type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export interface DesktopBridge {
  prepareApp(source: string): Promise<PermissionPreview>;
  activateApp(token: number, approvals: PermissionApproval[]): Promise<DesktopSnapshot>;
  dispatch(event: RuntimeDispatchEvent): Promise<DesktopSnapshot>;
}

/** Typed IPC facade; React never invokes Operations or native capabilities directly. */
export function createDesktopBridge(call: Invoke = invoke): DesktopBridge {
  return {
    prepareApp: source => call<PermissionPreview>('prepare_app', { source }),
    activateApp: (token, approvals) => call<DesktopSnapshot>('activate_app', { token, approvals }),
    dispatch: event => call<DesktopSnapshot>('dispatch_event', { event })
  };
}
