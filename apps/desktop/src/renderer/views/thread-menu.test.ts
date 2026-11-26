import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Bot } from '../../shared/models.ts'
import type { WorkItem, WorkStatus } from '../../shared/work.ts'
import { threadMenuItems, type ThreadMenuActions } from './thread-menu.ts'

const task = (id: string, status: WorkStatus, change: Partial<WorkItem> = {}): WorkItem => ({
  id, title: `Title ${id}`, status, activity: '', task: null, wait: null, cutIn: false, createdAt: 1000, startedAt: null, endedAt: null, result: '', source: null, worktree: null, folder: null, talk: false, agent: null, talkFiles: false, seat: null, runsOn: null, ...change,
})
const bot = (work: WorkItem[], change: Partial<Bot> = {}) => ({ id: 'b', work, maxAgents: 1, plain: false, ...change }) as Bot

/** The menu's titles, a `—` where a divider starts a part; and what each action did. */
function menu(b: Bot, t: WorkItem, canStart = true) {
  const did: string[] = []
  const log = (name: string) => () => did.push(name)
  const act: ThreadMenuActions = {
    open: log('open'), openChat: log('chat'), startAnother: log('new'), stop: log('stop'), cutIn: log('cutIn'),
    first: log('first'), retry: log('retry'), drop: log('drop'), copy: (text) => did.push(`copy ${text}`),
  }
  const items = threadMenuItems(b, t, canStart, act)
  return { titles: items.flatMap((i) => (i.divider ? ['—', i.title] : [i.title])), items, did }
}

test('a working thread opens, goes to its chat, starts another, stops and copies, and is never dropped while it runs', () => {
  const working = task('w', 'working', { task: 'th', folder: '/repo' })
  const { titles, items, did } = menu(bot([working]), working)
  assert.deepEqual(titles, ['Open thread', 'Open chat', 'New thread', '—', 'Stop', '—', 'Copy title', 'Copy folder path'])
  for (const item of items) item.action?.()
  assert.deepEqual(did, ['open', 'chat', 'new', 'stop', 'copy Title w', 'copy /repo'])
})

test('one that needs the user is answered in the chat', () => {
  const asking = task('a', 'needsYou', { task: 'th' })
  assert.deepEqual(menu(bot([asking]), asking).titles.slice(0, 2), ['Open thread', 'Answer in chat'])
})

test('a waiting one goes first, or cuts in when it is first and every place is taken, and can be dropped', () => {
  const working = task('w', 'working')
  const first = task('q1', 'waiting')
  const second = task('q2', 'waiting')
  const b = bot([working, first, second])
  assert.deepEqual(menu(b, second).titles, ['Open chat', 'New thread', '—', 'First', '—', 'Copy title', '—', 'Drop'])
  assert.deepEqual(menu(b, first).titles, ['Open chat', 'New thread', '—', 'Cut in', '—', 'Copy title', '—', 'Drop'])
  assert.equal(menu(b, second).items.at(-1)?.destructive, true)
})

test('an ended one is tried again unless another bot asked for it, and the box without a bot has no chat', () => {
  const failed = task('f', 'failed', { task: 'th' })
  assert.deepEqual(menu(bot([failed]), failed, false).titles, ['Open thread', 'Open chat', '—', 'Try again', '—', 'Copy title', '—', 'Drop'])
  const asked = task('r', 'failed', { source: { kind: 'request', id: 'other' } })
  assert.ok(!menu(bot([asked]), asked).titles.includes('Try again'))
  const plain = task('p', 'done', { task: 'th' })
  assert.ok(!menu(bot([plain], { plain: true }), plain).titles.includes('Open chat'))
})
