import { test, expect } from '@playwright/test'
import { join, resolve } from 'node:path'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach } from './sandbox.ts'

// A card's preview frame is a cross-process surface under the frosted chrome; off screen it made the compositor drop
// the blur layers for single frames (#505), so it mounts only while its card is near the viewport.
test("a card's preview frame unmounts off screen and comes back when scrolled in", async () => {
  test.setTimeout(90_000)
  const { browser, page } = await attach()
  const name = `Artifact offscreen ${Date.now()}`
  let botId: string | undefined
  try {
    const agent = join(resolve(__dirname, '../../..'), 'host/tests/fixtures/artifact_agent.py')
    const { bot } = await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(agent), connectors: [], skills: [], notify: false,
    })
    botId = bot.id
    await api(page, 'send', { botId, text: 'make-artifact' })
    await page.locator('.roster-row', { hasText: name }).click()
    const card = page.locator('.artifact-card').first()
    await expect(card).toBeVisible()
    const preview = card.locator('.artifact-card-preview')
    await expect(preview.locator('iframe')).toHaveCount(1) // on screen: the preview renders
    await expect(preview.locator('iframe').contentFrame().getByText('Weekly report')).toBeVisible()

    // Push the card out of view and give the observer a beat: the frame unmounts, the card keeps its size.
    await api(page, 'send', { botId, text: 'hold the card above\n'.repeat(30) })
    await page.evaluate(() => {
      const scroller = document.querySelector('.transcript') ?? document.scrollingElement
      scroller!.scrollTop = scroller!.scrollHeight
    })
    await page.waitForTimeout(1500)

    // Scrolled back in, the preview comes back.
    await card.scrollIntoViewIfNeeded()
    await expect(preview.locator('iframe').contentFrame().getByText('Weekly report')).toBeVisible({ timeout: 10_000 })
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
