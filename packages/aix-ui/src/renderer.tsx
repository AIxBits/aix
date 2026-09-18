import { Fragment, type ReactNode } from 'react';
import { isRecord, resolveUiValue } from './bindings.js';
import type { RenderSnapshot, ResourceHandle, UiEvent, UiNode } from './protocol.js';

export interface AixRendererProps {
  snapshot: RenderSnapshot;
  dispatch: (event: UiEvent) => void | Promise<void>;
}

/** Render a validated AIX UI tree into React without application business logic. */
export function AixRenderer({ snapshot, dispatch }: AixRendererProps) {
  return <NodeRenderer node={snapshot.ui} snapshot={snapshot} dispatch={dispatch} />;
}

interface NodeRendererProps extends AixRendererProps { node: UiNode }

function NodeRenderer({ node, snapshot, dispatch }: NodeRendererProps): ReactNode {
  const props = resolveProps(node.props, snapshot.state);
  const children = (node.children ?? []).map(child => (
    <Fragment key={child.id}><NodeRenderer node={child} snapshot={snapshot} dispatch={dispatch} /></Fragment>
  ));
  switch (node.type) {
    case 'page':
      return <main data-aix-id={node.id} aria-label={stringProp(props, 'label') ?? snapshot.appName}>{children}</main>;
    case 'container':
      return <section data-aix-id={node.id} aria-label={stringProp(props, 'label')}>{children}</section>;
    case 'text':
      return <p data-aix-id={node.id}>{textContent(props, snapshot.resources)}</p>;
    case 'button':
      return <button data-aix-id={node.id} type="button" disabled={booleanProp(props, 'disabled')} onClick={() => void dispatch({ type: 'ui.click', target: node.id, payload: {} })}>{stringProp(props, 'label') ?? textContent(props, snapshot.resources) ?? node.id}</button>;
    case 'input':
      return <input data-aix-id={node.id} aria-label={stringProp(props, 'label') ?? node.id} value={scalarText(props.value)} placeholder={stringProp(props, 'placeholder')} onChange={event => void dispatch({ type: 'ui.change', target: node.id, payload: { value: event.currentTarget.value } })} />;
    case 'list':
      return <ListNode id={node.id} value={dataValue(props, snapshot.resources)} />;
    case 'table':
      return <TableNode id={node.id} value={dataValue(props, snapshot.resources)} columns={props.columns} />;
    case 'image':
      return <MediaNode kind="image" id={node.id} props={props} resources={snapshot.resources} />;
    case 'audio':
      return <MediaNode kind="audio" id={node.id} props={props} resources={snapshot.resources} />;
    case 'video':
      return <MediaNode kind="video" id={node.id} props={props} resources={snapshot.resources} />;
  }
}

function resolveProps(props: Record<string, unknown> | undefined, state: Record<string, unknown>): Record<string, unknown> {
  return resolveUiValue(props ?? {}, state) as Record<string, unknown>;
}

function textContent(props: Record<string, unknown>, resources: Record<string, ResourceHandle>): string {
  const resource = resourceFor(props, resources);
  if (resource?.type === 'text') return resource.text;
  return scalarText(props.text);
}

function dataValue(props: Record<string, unknown>, resources: Record<string, ResourceHandle>): unknown {
  const resource = resourceFor(props, resources);
  return resource?.type === 'data' ? resource.data : props.items ?? props.rows ?? [];
}

function resourceFor(props: Record<string, unknown>, resources: Record<string, ResourceHandle>): ResourceHandle | undefined {
  const id = stringProp(props, 'resource');
  return id ? resources[id] : undefined;
}

function ListNode({ id, value }: { id: string; value: unknown }) {
  const items = Array.isArray(value) ? value : [];
  return <ul data-aix-id={id}>{items.map((item, index) => <li key={index}>{displayValue(item)}</li>)}</ul>;
}

function TableNode({ id, value, columns }: { id: string; value: unknown; columns: unknown }) {
  const rows = Array.isArray(value) ? value.filter(isRecord) : [];
  const selected = Array.isArray(columns) && columns.every(column => typeof column === 'string')
    ? columns as string[] : [...new Set(rows.flatMap(row => Object.keys(row)))];
  return <table data-aix-id={id}><thead><tr>{selected.map(column => <th key={column} scope="col">{column}</th>)}</tr></thead><tbody>{rows.map((row, index) => <tr key={index}>{selected.map(column => <td key={column}>{displayValue(row[column])}</td>)}</tr>)}</tbody></table>;
}

function MediaNode({ kind, id, props, resources }: { kind: 'image' | 'audio' | 'video'; id: string; props: Record<string, unknown>; resources: Record<string, ResourceHandle> }) {
  const handle = resourceFor(props, resources);
  if (!handle || handle.type !== kind) return <span data-aix-id={id} role="status">Resource unavailable</span>;
  if (kind === 'image') return <img data-aix-id={id} src={handle.uri} alt={stringProp(props, 'alt') ?? ''} />;
  if (kind === 'audio') return <audio data-aix-id={id} src={handle.uri} controls />;
  return <video data-aix-id={id} src={handle.uri} controls />;
}

function stringProp(props: Record<string, unknown>, key: string): string | undefined { return typeof props[key] === 'string' ? props[key] : undefined }
function booleanProp(props: Record<string, unknown>, key: string): boolean { return props[key] === true }
function scalarText(value: unknown): string { return value === null || value === undefined ? '' : ['string', 'number', 'boolean'].includes(typeof value) ? String(value) : '' }
function displayValue(value: unknown): string { return value === null || value === undefined ? '' : ['string', 'number', 'boolean'].includes(typeof value) ? String(value) : JSON.stringify(value) }
