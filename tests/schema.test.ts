import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parseApp, validateApp } from '../packages/aix-schema/src/index.ts';
const fixture = () => JSON.parse(readFileSync(new URL('../examples/hello.aix.json', import.meta.url), 'utf8'));
test('accepts minimal example', () => assert.equal(validateApp(fixture()).valid, true));
test('JSON and YAML parsing', () => { assert.deepEqual(parseApp('city: Shanghai'), { city: 'Shanghai' }); assert.equal(validateApp(parseApp(JSON.stringify(fixture()))).valid, true); });
test('rejects unsupported versions, root fields and UI kinds', () => {
  for (const mutate of [(a: any) => a.specVersion = '9', (a: any) => a.script = 'shell', (a: any) => a.ui.type = 'script']) {
    const app = fixture(); mutate(app); assert.equal(validateApp(app).valid, false);
  }
});
test('requires scoped known capabilities', () => {
  const app = fixture();
  app.permissions = [{ capability: 'ai.generate', scopes: ['default'] }];
  assert.equal(validateApp(app).valid, true);
  for (const p of [{ capability: 'admin', scopes: ['*'] }, { capability: 'file.read', scopes: [] }]) {
    app.permissions = [p]; assert.equal(validateApp(app).valid, false);
  }
});
test('rejects duplicate UI ids', () => { const a = fixture(); a.ui.children.push(a.ui.children[0]); assert.equal(validateApp(a).valid, false); });
test('checks workflow graph and event targets', () => {
  const a = fixture();
  a.workflows = [{ id: 'refresh', on: { type: 'ui.click', target: 'city' }, entry: 'read', steps: [{ id: 'read', operation: 'state.get', input: { path: 'city' }, next: [] }] }];
  assert.equal(validateApp(a).valid, true);
  a.workflows[0].steps[0].next = ['read']; assert.equal(validateApp(a).valid, false);
  a.workflows[0].steps[0].next = ['missing']; assert.equal(validateApp(a).valid, false);
  a.workflows[0].steps[0].next = []; a.workflows[0].on.target = 'missing'; assert.equal(validateApp(a).valid, false);
});
test('accepts conditional workflow edges and bounds graph size', () => {
  const a = fixture();
  a.workflows = [{ id: 'branch', on: { type: 'app.start' }, entry: 'choose', steps: [
    { id: 'choose', operation: 'logic.if', input: { condition: true, then: 1, else: 0 }, next: [{ step: 'done', when: { path: '/value', equals: 1 } }] },
    { id: 'done', operation: 'state.set', input: { path: '/result', value: 1 } }
  ] }];
  assert.equal(validateApp(a).valid, true);
  a.workflows[0].steps[0].next[0].step = 'missing';
  assert.equal(validateApp(a).valid, false);
  a.workflows[0].steps = Array.from({ length: 257 }, (_, index) => ({ id: `step${index}`, operation: 'time.now', input: {} }));
  a.workflows[0].entry = 'step0';
  assert.equal(validateApp(a).valid, false);
});
test('rejects timer without interval', () => { const a = fixture(); a.workflows = [{id:'tick',on:{type:'timer'},entry:'n',steps:[{id:'n',operation:'time.now',input:{}}]}]; assert.equal(validateApp(a).valid,false); });
test('rejects duplicate YAML keys, aliases and oversized input', () => {
  assert.throws(() => parseApp('a: 1\na: 2'));
  assert.throws(() => parseApp('a: &a [1]\nb: *a'));
  assert.throws(() => parseApp('a'.repeat(1_048_577)));
});
test('YAML example matches JSON example', () => {
  const yaml = parseApp(readFileSync(new URL('../examples/hello.aix.yaml', import.meta.url), 'utf8'));
  assert.deepEqual(yaml, fixture());
  assert.equal(validateApp(yaml).valid, true);
});
test('rejects duplicate connector operation ids and invalid protocols', () => {
  const a = fixture();
  a.connectors = [{id:'weather',type:'http',baseUrl:'https://example.com',operations:[{id:'weather.get',method:'GET',path:'/weather'}]}];
  assert.equal(validateApp(a).valid, true);
  a.connectors[0].operations.push(a.connectors[0].operations[0]);
  assert.equal(validateApp(a).valid, false);
  a.connectors[0].operations.pop(); a.connectors[0].baseUrl = 'file:///etc/passwd';
  assert.equal(validateApp(a).valid, false);
});
