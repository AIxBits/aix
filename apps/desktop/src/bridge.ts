import { invoke } from '@tauri-apps/api/core';
import type { RenderSnapshot, TimerSubscription, UiEvent } from '@aix/ui';

export interface DesktopSnapshot extends RenderSnapshot { timers: TimerSubscription[] }
export interface PermissionRequest { capability: string; scopes: string[] }
export interface PermissionPreview { token: number; appId: string; appName: string; requests: PermissionRequest[] }
export interface PermissionApproval { capability: string; scopes: string[] }
export type RuntimeDispatchEvent = UiEvent | { type: 'timer'; workflowId: string; payload: null };
export type ProviderKind = 'open_ai_compatible' | 'local_open_ai_compatible';
export interface ProviderProfile { id: string; name: string; kind: ProviderKind; baseUrl: string; model: string; hasApiKey: boolean }
export interface ProviderProfileInput { id: string; name: string; kind: ProviderKind; baseUrl: string; model: string; apiKey?: string }
export interface GenerateDefinitionInput { profileId: string; requirement: string; existingSource?: string }
export interface DefinitionChange { path: string; kind: 'added' | 'removed' | 'changed' }
export interface GeneratedDefinition { appId: string; appName: string; source: string; attempts: number; permissions: PermissionRequest[]; changes: DefinitionChange[] }
export type ExportFormat = 'json' | 'yaml';
export interface SavedDefinition { path: string }
export type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export interface DesktopBridge {
  prepareApp(source: string): Promise<PermissionPreview>;
  activateApp(token: number, approvals: PermissionApproval[]): Promise<DesktopSnapshot>;
  dispatch(event: RuntimeDispatchEvent): Promise<DesktopSnapshot>;
  listProviderProfiles(): Promise<ProviderProfile[]>;
  saveProviderProfile(input: ProviderProfileInput): Promise<ProviderProfile>;
  deleteProviderProfile(profileId: string): Promise<void>;
  generateApp(input: GenerateDefinitionInput): Promise<GeneratedDefinition>;
  saveGeneratedDefinition(source: string, format: ExportFormat): Promise<SavedDefinition>;
}

/** Typed IPC facade; React never invokes Operations or native capabilities directly. */
export function createDesktopBridge(call: Invoke = invoke): DesktopBridge {
  return {
    prepareApp: source => call<PermissionPreview>('prepare_app', { source }),
    activateApp: (token, approvals) => call<DesktopSnapshot>('activate_app', { token, approvals }),
    dispatch: event => call<DesktopSnapshot>('dispatch_event', { event }),
    listProviderProfiles: () => call<ProviderProfile[]>('list_provider_profiles'),
    saveProviderProfile: input => call<ProviderProfile>('save_provider_profile', { input }),
    deleteProviderProfile: profileId => call<void>('delete_provider_profile', { profileId }),
    generateApp: input => call<GeneratedDefinition>('generate_app', { input }),
    saveGeneratedDefinition: (source, format) => call<SavedDefinition>('save_generated_definition', { source, format })
  };
}
