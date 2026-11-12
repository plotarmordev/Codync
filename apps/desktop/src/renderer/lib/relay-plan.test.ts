import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { CloudComputer } from '../store/cloud'
import { relayPlan } from './relay-plan.ts'

const computer = (id: string, access: string | null): CloudComputer => ({ computerId: id, name: id, signKey: '', access })
const stores = (entries: Record<string, boolean>) => (id: string) => (id in entries ? { isLoopback: entries[id]! } : null)

test('approved computers open through the relay, except this one, a Codync Cloud one and one already reached over SSH', () => {
  const cloud: CloudComputer = { ...computer('cloud', 'granted'), cloud: { provider: 'p', providerName: 'P', region: 'r', state: 'running' } }
  const plan = relayPlan([computer('here', 'granted'), computer('studio', 'granted'), cloud, computer('ssh', 'granted'), computer('asked', 'pending'), computer('new', 'none')], 'here', new Set(), stores({ here: true, ssh: true }))
  assert.deepEqual(plan.attach.map((c) => c.computerId), ['studio'])
  assert.deepEqual(plan.detach, [])
})

test('a relay store whose grant went is detached; one SSH took over keeps its own store', () => {
  const plan = relayPlan([computer('kept', 'granted'), computer('ssh', 'granted')], null, new Set(['kept', 'revoked', 'ssh']), stores({ kept: false, revoked: false, ssh: true }))
  assert.deepEqual(plan.attach, [])
  assert.deepEqual(plan.detach, ['revoked'])
  assert.deepEqual([...plan.kept], ['kept'])
})

test('a relay store that turns out to be this computer is let go: its local host reaches it', () => {
  const plan = relayPlan([computer('here', 'granted')], 'here', new Set(['here']), stores({ here: false }))
  assert.deepEqual(plan.detach, ['here'])
})

test('when the SSH tunnel ends, the approved computer opens through the relay again', () => {
  const plan = relayPlan([computer('ssh', 'granted')], null, new Set(), stores({}))
  assert.deepEqual(plan.attach.map((c) => c.computerId), ['ssh'])
})
