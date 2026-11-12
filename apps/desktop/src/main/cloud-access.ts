import { randomBytes } from 'node:crypto'
import { ipcMain } from 'electron'
import type { AccessTicket, AccountComputer } from '../shared/ipc'
import type { AccountService } from './account'
import { b64url, computerIdRaw, sasCode, sasCommit, unb64url } from './channel/crypto.ts'
import { cloudRequest, identity, type CloudFailure } from './cloud'

// Asking an account computer for access (docs/reference/remote-relay.md §4.2 B): commit to a nonce, reveal it
// once the computer answered with its own. A Codync Cloud computer approves this account's devices by itself, so
// nobody compares the code (`requestAccess`); any other computer shows the code to a person, who approves it there
// only if this app shows the same (`askForAccess`). The nonce never leaves this file before the reveal.

const POLL_MS = 1500

type Status = { status: string; hostNonce?: string; expiresAt?: number }
type Result<T> = { ok: true; value: T } | { ok: false; error: CloudFailure }

/** An access request sent to `computerId`, with the nonce it committed to and this device's public key. */
async function open(account: AccountService, computerId: string) {
  const user = account.state.user
  if (!user) return { ok: false, error: { status: 0, code: 'signedOut', message: 'Sign in again to reach your account.' } } as const
  const nD = randomBytes(32)
  const device = unb64url((await identity(user.userId)).publicKey)
  const created = await cloudRequest(account, 'POST', `v1/computers/${computerId}/access-requests`, { commit: b64url(sasCommit(device, nD)) }, true)
  if (!created.ok) return created
  return { ok: true, value: { nD, device, requestId: String(created.value.requestId), expiresAt: Number(created.value.expiresAt) } } as const
}

/** The request's status once `done` accepts it, or a settled one; null once it expired here. */
async function wait(account: AccountService, requestId: string, expiresAt: number, done: (s: Status) => boolean): Promise<Result<Status> | null> {
  while (Date.now() < expiresAt) {
    const res = await cloudRequest(account, 'GET', `v1/access-requests/${requestId}`, undefined, false)
    if (!res.ok) return res
    const s = res.value as unknown as Status
    if (s.status !== 'pending' || done(s)) return { ok: true, value: s }
    await new Promise((r) => setTimeout(r, POLL_MS))
  }
  return null
}

const reveal = (account: AccountService, requestId: string, nD: Buffer) =>
  cloudRequest(account, 'POST', `v1/access-requests/${requestId}/reveal`, { nonce: b64url(nD) }, true)

/** Takes the request back, so the computer's list doesn't keep it. */
const withdraw = (account: AccountService, requestId: string) => cloudRequest(account, 'DELETE', `v1/access-requests/${requestId}`, undefined, false)

async function requestAccess(account: AccountService, computerId: string): Promise<Result<{ status: string }>> {
  const opened = await open(account, computerId)
  if (!opened.ok) return opened
  const { nD, requestId, expiresAt } = opened.value
  const answered = await wait(account, requestId, expiresAt, (s) => !!s.hostNonce)
  if (answered?.ok && answered.value.status === 'pending') {
    const revealed = await reveal(account, requestId, nD)
    if (!revealed.ok) return revealed
    const decided = await wait(account, requestId, expiresAt, () => false)
    if (decided) return decided.ok ? { ok: true, value: { status: decided.value.status } } : decided
  } else if (answered) {
    return answered.ok ? { ok: true, value: { status: answered.value.status } } : answered
  }
  await withdraw(account, requestId)
  return { ok: true, value: { status: 'expired' } }
}

/**
 * Asks `computer` to let this install in (iOS and Android `requestAccess`) and returns the 6-digit code both sides
 * show, once the computer answered; the renderer then waits for the person's decision there.
 */
async function askForAccess(account: AccountService, computer: AccountComputer): Promise<Result<AccessTicket>> {
  const refused = (message: string): Result<AccessTicket> => ({ ok: false, error: { status: 0, code: 'refused', message } })
  const hostKey = unb64url(computer.signKey)
  if (b64url(computerIdRaw(hostKey)) !== computer.id) return refused("This computer's key doesn't match its ID.")
  const opened = await open(account, computer.id)
  if (!opened.ok) return opened
  const { nD, device, requestId, expiresAt } = opened.value
  const answered = await wait(account, requestId, expiresAt, (s) => !!s.hostNonce)
  const hostNonce = answered?.ok && answered.value.status === 'pending' ? answered.value.hostNonce : undefined
  const revealed = hostNonce ? await reveal(account, requestId, nD) : null
  if (hostNonce && revealed?.ok) return { ok: true, value: { requestId, expiresAt, code: sasCode(hostKey, device, nD, unb64url(hostNonce)) } }
  // A failed nonce exchange leaves no invisible request waiting at the computer.
  await withdraw(account, requestId)
  if (revealed && !revealed.ok) return revealed
  if (answered && !answered.ok) return answered
  return refused(answered ? "The computer didn't take the request." : 'The request expired. Ask again.')
}

export function registerCloudAccess(account: AccountService) {
  ipcMain.handle('cloud:requestAccess', (_e, computerId: string) => requestAccess(account, computerId))
  ipcMain.handle('cloud:askForAccess', (_e, computer: AccountComputer) => askForAccess(account, computer))
}
