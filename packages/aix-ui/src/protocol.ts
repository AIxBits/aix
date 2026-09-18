/** Framework-neutral UI node received from the validated Runtime snapshot. */
export interface UiNode {
  id: string;
  type: 'page' | 'container' | 'text' | 'button' | 'input' | 'list' | 'table' | 'image' | 'audio' | 'video';
  props?: Record<string, unknown>;
  children?: UiNode[];
}

/** A resource already resolved by the trusted host. The renderer never fetches definitions directly. */
export type ResourceHandle =
  | { type: 'text'; text: string }
  | { type: 'data'; data: unknown }
  | { type: 'image' | 'audio' | 'video'; uri: string; mimeType?: string }
  | { type: 'unavailable'; reason: string };

/** Immutable data consumed by one render pass. */
export interface RenderSnapshot {
  appId: string;
  appName: string;
  ui: UiNode;
  state: Record<string, unknown>;
  resources: Record<string, ResourceHandle>;
}

/** UI events returned to the Runtime event dispatcher. */
export type UiEvent =
  | { type: 'ui.click'; target: string; payload: Record<string, unknown> }
  | { type: 'ui.change'; target: string; payload: { value: string } };

/** Timer subscription owned and scheduled by the desktop host. */
export interface TimerSubscription { workflowId: string; intervalMs: number }
