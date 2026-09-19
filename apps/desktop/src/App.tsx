import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AixRenderer, type UiEvent } from '@aix/ui';
import {
  createDesktopBridge, type DesktopBridge, type DesktopSnapshot, type ExportFormat,
  type GeneratedDefinition, type PermissionPreview, type ProviderKind,
  type ProviderProfile, type RuntimeDispatchEvent
} from './bridge.js';

type HomeMode = 'builder' | 'loader';

export function App({ bridge: providedBridge }: { bridge?: DesktopBridge }) {
  const bridge = useMemo(() => providedBridge ?? createDesktopBridge(), [providedBridge]);
  const [mode, setMode] = useState<HomeMode>('builder');
  const [source, setSource] = useState('');
  const [snapshot, setSnapshot] = useState<DesktopSnapshot>();
  const [preview, setPreview] = useState<PermissionPreview>();
  const [approved, setApproved] = useState<Set<string>>(new Set());
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [profiles, setProfiles] = useState<ProviderProfile[]>([]);
  const [selectedProfile, setSelectedProfile] = useState('');
  const [editingProfile, setEditingProfile] = useState(false);
  const [requirement, setRequirement] = useState('');
  const [generated, setGenerated] = useState<GeneratedDefinition>();
  const [savedPath, setSavedPath] = useState('');
  const requestSequence = useRef(0);

  const refreshProfiles = useCallback(async () => {
    try {
      const items = await bridge.listProviderProfiles();
      setProfiles(items);
      setSelectedProfile(current => current && items.some(item => item.id === current) ? current : (items[0]?.id ?? ''));
      if (items.length === 0) setEditingProfile(true);
    } catch (reason) { setError(errorMessage(reason)); }
  }, [bridge]);

  useEffect(() => { void refreshProfiles(); }, [refreshProfiles]);

  async function load(candidate = source) {
    const request = ++requestSequence.current;
    setBusy(true); setError('');
    try {
      const next = await bridge.prepareApp(candidate);
      if (request !== requestSequence.current) return;
      setSource(candidate);
      if (next.requests.length === 0) setSnapshot(await bridge.activateApp(next.token, []));
      else { setPreview(next); setApproved(new Set()); }
    } catch (reason) {
      if (request === requestSequence.current) setError(errorMessage(reason));
    } finally { if (request === requestSequence.current) setBusy(false); }
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
    } finally { if (request === requestSequence.current) setBusy(false); }
  }

  async function generate() {
    if (!selectedProfile || !requirement.trim()) return;
    const request = ++requestSequence.current;
    setBusy(true); setError(''); setSavedPath('');
    try {
      const result = await bridge.generateApp({
        profileId: selectedProfile,
        requirement,
        ...(generated ? { existingSource: generated.source } : {})
      });
      if (request === requestSequence.current) { setGenerated(result); setSource(result.source); }
    } catch (reason) {
      if (request === requestSequence.current) setError(errorMessage(reason));
    } finally { if (request === requestSequence.current) setBusy(false); }
  }

  async function save(format: ExportFormat) {
    if (!generated) return;
    setError(''); setSavedPath('');
    try { setSavedPath((await bridge.saveGeneratedDefinition(generated.source, format)).path); }
    catch (reason) { setError(errorMessage(reason)); }
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

  function returnHome() {
    requestSequence.current += 1;
    setSnapshot(undefined); setPreview(undefined); setApproved(new Set()); setError('');
  }

  return <div className="shell">
    <header>
      <strong>AIX Runtime</strong><span>{snapshot?.appName ?? 'Build or load an App Definition'}</span>
      {snapshot && <button type="button" className="secondary" onClick={returnHome}>Home</button>}
    </header>
    {!snapshot && !preview && <>
      <nav className="mode-tabs" aria-label="App source">
        <button type="button" className={mode === 'builder' ? 'active' : 'secondary'} onClick={() => setMode('builder')}>Build with AI</button>
        <button type="button" className={mode === 'loader' ? 'active' : 'secondary'} onClick={() => setMode('loader')}>Load JSON</button>
      </nav>
      {mode === 'builder' && <section className="builder" aria-label="AI App Builder">
        <div className="section-heading"><div><h2>AI App Builder</h2><p>The model creates data. AIX validates it before you can run it.</p></div><button type="button" className="secondary" onClick={() => setEditingProfile(current => !current)}>{editingProfile ? 'Close provider setup' : 'Provider setup'}</button></div>
        {editingProfile && <ProviderEditor bridge={bridge} profiles={profiles} onChanged={refreshProfiles} />}
        <label htmlFor="provider">Provider profile</label>
        <select id="provider" value={selectedProfile} onChange={event => setSelectedProfile(event.currentTarget.value)}><option value="">Select a provider</option>{profiles.map(profile => <option key={profile.id} value={profile.id}>{profile.name} · {profile.model}</option>)}</select>
        <label htmlFor="requirement">Describe the app you want</label>
        <textarea className="requirement" id="requirement" value={requirement} onChange={event => setRequirement(event.currentTarget.value)} placeholder="Create an app that…" />
        <button type="button" disabled={busy || !selectedProfile || !requirement.trim()} onClick={() => void generate()}>{busy ? 'Generating and validating…' : generated ? 'Revise definition' : 'Generate definition'}</button>
        {generated && <GeneratedReview generated={generated} savedPath={savedPath} onRun={() => void load(generated.source)} onSave={format => void save(format)} />}
      </section>}
      {mode === 'loader' && <section className="loader" aria-label="App loader">
        <label htmlFor="definition">AIX App Definition (JSON)</label>
        <textarea id="definition" value={source} onChange={event => setSource(event.currentTarget.value)} spellCheck={false} />
        <button type="button" disabled={busy || source.trim() === ''} onClick={() => void load()}>{busy ? 'Checking…' : 'Review and load'}</button>
      </section>}
    </>}
    {!snapshot && preview && <section className="permissions" aria-label="Permission review">
      <h2>Permissions requested by {preview.appName}</h2><p>Select only the scopes you want to grant. Unselected requests remain denied.</p>
      {preview.requests.flatMap(item => item.scopes.map(scope => {
        const key = `${item.capability}\n${scope}`;
        return <label className="permission" key={key}><input type="checkbox" checked={approved.has(key)} onChange={event => setApproved(current => { const next = new Set(current); event.currentTarget.checked ? next.add(key) : next.delete(key); return next; })} /><span><strong>{item.capability}</strong><code>{scope}</code></span></label>;
      }))}
      <div className="actions"><button type="button" disabled={busy} onClick={() => void activate()}>{busy ? 'Loading…' : 'Load with selected permissions'}</button><button type="button" className="secondary" disabled={busy} onClick={() => setPreview(undefined)}>Cancel</button></div>
    </section>}
    {error && <p className="error" role="alert">{error}</p>}
    {snapshot && <AixRenderer snapshot={snapshot} dispatch={dispatch} />}
  </div>;
}

function ProviderEditor({ bridge, profiles, onChanged }: { bridge: DesktopBridge; profiles: ProviderProfile[]; onChanged: () => Promise<void> }) {
  const [id, setId] = useState('default');
  const [name, setName] = useState('My model');
  const [kind, setKind] = useState<ProviderKind>('open_ai_compatible');
  const [baseUrl, setBaseUrl] = useState('https://api.openai.com/v1');
  const [model, setModel] = useState('gpt-4.1-mini');
  const [message, setMessage] = useState('');
  const keyRef = useRef<HTMLInputElement>(null);

  async function submit(event: React.FormEvent) {
    event.preventDefault(); setMessage('');
    const apiKey = keyRef.current?.value.trim();
    try {
      await bridge.saveProviderProfile({ id, name, kind, baseUrl, model, ...(apiKey ? { apiKey } : {}) });
      if (keyRef.current) keyRef.current.value = '';
      setMessage('Provider profile saved. The key is stored by the operating system.');
      await onChanged();
    } catch (reason) { setMessage(errorMessage(reason)); }
  }

  async function remove(profileId: string) {
    try { await bridge.deleteProviderProfile(profileId); await onChanged(); setMessage('Provider profile deleted.'); }
    catch (reason) { setMessage(errorMessage(reason)); }
  }

  return <form className="provider-editor" onSubmit={event => void submit(event)}>
    <h3>Provider profiles</h3><div className="form-grid">
      <label>Profile ID<input value={id} onChange={event => setId(event.currentTarget.value)} pattern="[a-z0-9-]+" required /></label>
      <label>Name<input value={name} onChange={event => setName(event.currentTarget.value)} required /></label>
      <label>Type<select value={kind} onChange={event => setKind(event.currentTarget.value as ProviderKind)}><option value="open_ai_compatible">OpenAI-compatible</option><option value="local_open_ai_compatible">Local compatible server</option></select></label>
      <label>Model<input value={model} onChange={event => setModel(event.currentTarget.value)} required /></label>
      <label className="wide">API base URL<input value={baseUrl} onChange={event => setBaseUrl(event.currentTarget.value)} type="url" required /></label>
      <label className="wide">API key <small>(leave empty to preserve the saved key)</small><input ref={keyRef} type="password" autoComplete="new-password" /></label>
    </div><button type="submit">Save provider</button>
    {profiles.length > 0 && <ul className="profile-list">{profiles.map(profile => <li key={profile.id}><span>{profile.name} <code>{profile.id}</code>{profile.hasApiKey ? ' · key stored' : ''}</span><button type="button" className="danger" onClick={() => void remove(profile.id)}>Delete</button></li>)}</ul>}
    {message && <p className="inline-message" role="status">{message}</p>}
  </form>;
}

function GeneratedReview({ generated, savedPath, onRun, onSave }: { generated: GeneratedDefinition; savedPath: string; onRun: () => void; onSave: (format: ExportFormat) => void }) {
  return <section className="generated-review" aria-label="Generated definition review">
    <div className="section-heading"><div><h3>{generated.appName}</h3><p>Validated after {generated.attempts} provider call{generated.attempts === 1 ? '' : 's'}.</p></div><span className="valid-badge">Runtime valid</span></div>
    <h4>Definition changes</h4><ul className="changes">{generated.changes.map((change, index) => <li key={`${change.path}-${index}`}><span className={`change-${change.kind}`}>{change.kind}</span><code>{change.path}</code></li>)}</ul>
    <h4>Requested permissions</h4>{generated.permissions.length === 0 ? <p>None</p> : <ul>{generated.permissions.map(permission => <li key={permission.capability}><strong>{permission.capability}</strong>: {permission.scopes.join(', ')}</li>)}</ul>}
    <details><summary>Validated JSON</summary><pre>{generated.source}</pre></details>
    <div className="actions"><button type="button" onClick={onRun}>Review permissions and run</button><button type="button" className="secondary" onClick={() => onSave('json')}>Save JSON</button><button type="button" className="secondary" onClick={() => onSave('yaml')}>Save YAML</button></div>
    {savedPath && <p className="saved-path" role="status">Saved to <code>{savedPath}</code></p>}
  </section>;
}

function errorMessage(reason: unknown): string {
  if (typeof reason === 'string') return reason;
  if (reason && typeof reason === 'object' && 'message' in reason && typeof reason.message === 'string') return reason.message;
  return 'The Runtime rejected the request';
}
