import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AixRenderer, type UiEvent } from '@aix/ui';
import { createDesktopBridge, type DesktopBridge, type DesktopSnapshot, type RuntimeDispatchEvent } from './bridge.js';

export function App({ bridge: providedBridge }: { bridge?: DesktopBridge }) {
  const bridge = useMemo(() => providedBridge ?? createDesktopBridge(), [providedBridge]);
  const [source, setSource] = useState('');
  const [snapshot, setSnapshot] = useState<DesktopSnapshot>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const requestSequence = useRef(0);

  async function load() {
    const request = ++requestSequence.current;
    setBusy(true); setError('');
    try {
      const next = await bridge.loadApp(source);
      if (request === requestSequence.current) setSnapshot(next);
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
      {!snapshot && <section className="loader" aria-label="App loader">
        <label htmlFor="definition">AIX App Definition (JSON)</label>
        <textarea id="definition" value={source} onChange={event => setSource(event.currentTarget.value)} spellCheck={false} />
        <button type="button" disabled={busy || source.trim() === ''} onClick={() => void load()}>{busy ? 'Loading…' : 'Load app'}</button>
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
