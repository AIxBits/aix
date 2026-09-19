import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AixRenderer, type UiEvent } from '@aix/ui';
import { createDesktopBridge, type DesktopBridge, type DesktopSnapshot, type PermissionPreview, type RuntimeDispatchEvent } from './bridge.js';

export function App({ bridge: providedBridge }: { bridge?: DesktopBridge }) {
  const bridge = useMemo(() => providedBridge ?? createDesktopBridge(), [providedBridge]);
  const [source, setSource] = useState('');
  const [snapshot, setSnapshot] = useState<DesktopSnapshot>();
  const [preview, setPreview] = useState<PermissionPreview>();
  const [approved, setApproved] = useState<Set<string>>(new Set());
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const requestSequence = useRef(0);

  async function load() {
    const request = ++requestSequence.current;
    setBusy(true); setError('');
    try {
      const next = await bridge.prepareApp(source);
      if (request !== requestSequence.current) return;
      if (next.requests.length === 0) setSnapshot(await bridge.activateApp(next.token, []));
      else { setPreview(next); setApproved(new Set()); }
    } catch (reason) {
      if (request === requestSequence.current) setError(errorMessage(reason));
    } finally {
      if (request === requestSequence.current) setBusy(false);
    }
  }

  async function activate() {
    if (!preview) return;
    const request = ++requestSequence.current;
    setBusy(true); setError('');
    try {
      const approvals = preview.requests.map(item => ({
        capability: item.capability,
        scopes: item.scopes.filter(scope => approved.has(`${item.capability}\n${scope}`))
      })).filter(item => item.scopes.length > 0);
      const next = await bridge.activateApp(preview.token, approvals);
      if (request === requestSequence.current) { setSnapshot(next); setPreview(undefined); }
    } catch (reason) {
      if (request === requestSequence.current) setError(errorMessage(reason));
    } finally {
      if (request === requestSequence.current) setBusy(false);
    }
  }

  const dispatchRuntime = useCallback(async (event: RuntimeDispatchEvent) => {
    const request = ++requestSequence.current;
    setError('');
    try {
      const next = await bridge.dispatch(event);
      if (request === requestSequence.current) setSnapshot(next);
    } catch (reason) {
      if (request === requestSequence.current) setError(errorMessage(reason));
    }
  }, [bridge]);

  const dispatch = useCallback((event: UiEvent) => dispatchRuntime(event), [dispatchRuntime]);

  useEffect(() => {
    if (!snapshot) return;
    const timers = snapshot.timers.map(timer => window.setInterval(() => {
      void dispatchRuntime({ type: 'timer', workflowId: timer.workflowId, payload: null });
    }, timer.intervalMs));
    return () => timers.forEach(timer => window.clearInterval(timer));
  }, [dispatchRuntime, snapshot?.appId, snapshot?.timers]);

  return (
    <div className="shell">
      <header>
        <strong>AIX Runtime</strong>
        <span>{snapshot?.appName ?? 'Load an App Definition'}</span>
        {snapshot && <button type="button" className="secondary" onClick={() => { requestSequence.current += 1; setSnapshot(undefined); }}>Load another</button>}
      </header>
      {!snapshot && !preview && <section className="loader" aria-label="App loader">
        <label htmlFor="definition">AIX App Definition (JSON)</label>
        <textarea id="definition" value={source} onChange={event => setSource(event.currentTarget.value)} spellCheck={false} />
        <button type="button" disabled={busy || source.trim() === ''} onClick={() => void load()}>{busy ? 'Checking…' : 'Review and load'}</button>
      </section>}
      {!snapshot && preview && <section className="permissions" aria-label="Permission review">
        <h2>Permissions requested by {preview.appName}</h2>
        <p>Select only the scopes you want to grant. Unselected requests remain denied.</p>
        {preview.requests.flatMap(item => item.scopes.map(scope => {
          const key = `${item.capability}\n${scope}`;
          return <label className="permission" key={key}>
            <input type="checkbox" checked={approved.has(key)} onChange={event => setApproved(current => {
              const next = new Set(current); event.currentTarget.checked ? next.add(key) : next.delete(key); return next;
            })} />
            <span><strong>{item.capability}</strong><code>{scope}</code></span>
          </label>;
        }))}
        <div className="actions">
          <button type="button" disabled={busy} onClick={() => void activate()}>{busy ? 'Loading…' : 'Load with selected permissions'}</button>
          <button type="button" className="secondary" disabled={busy} onClick={() => setPreview(undefined)}>Cancel</button>
        </div>
      </section>}
      {error && <p className="error" role="alert">{error}</p>}
      {snapshot && <AixRenderer snapshot={snapshot} dispatch={dispatch} />}
    </div>
  );
}

function errorMessage(reason: unknown): string {
  if (typeof reason === 'string') return reason;
  if (reason && typeof reason === 'object' && 'message' in reason && typeof reason.message === 'string') return reason.message;
  return 'The Runtime rejected the request';
}
