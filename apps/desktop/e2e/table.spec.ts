import { test, expect } from '@playwright/test'
import { api, attach, echoBot, say } from './sandbox.ts'

// A table's cells wrap inside the bubble instead of running past it; a short table keeps its
// compact size. The echo bot prefixes its answer with its mode, so each message opens with a
// paragraph line for the table to parse.
const LONG = 'A table:\n\n| Name | Status |\n|---|---|\n| Alice | The version manager keeps every toolchain on this computer up to date, and it checks for new releases every hour |\n| Bob | Busy |'
const SHORT = 'A table:\n\n| Name | Status |\n|---|---|\n| Alice | Ready |\n| Bob | Busy |'

test('a table wraps its cells inside the bubble', async () => {
  const { browser, page } = await attach()
  let botId: string | undefined
  try {
    const skip = page.getByRole('button', { name: /Continue without an account/ })
    if (await skip.isVisible()) await skip.click()
    const name = `Table ${Date.now()}`
    const bot = await echoBot(page, name)
    botId = bot.id
    await say(page, bot.id, LONG)
    await say(page, bot.id, SHORT)
    await page.locator('.roster-row', { hasText: name }).click()
    // Only the bot's answers render the table; the sent text shows as written.
    await expect(page.locator('.md-table')).toHaveCount(2)

    const tables = await page.evaluate(() => {
      const box = (el: Element) => el.getBoundingClientRect()
      return [...document.querySelectorAll('.md-table')].map((table) => {
        const bubble = box(table.closest('.bubble')!)
        const cells = [...table.children].filter((c) => c.tagName === 'SPAN')
        const long = cells.find((c) => c.textContent!.startsWith('The version'))
        return {
          inside: cells.every((c) => box(c).right <= bubble.right + 0.5),
          wrapped: long ? box(long).height > parseFloat(getComputedStyle(long).lineHeight) * 1.5 : false,
          singleLine: cells.every((c) => box(c).height <= parseFloat(getComputedStyle(c).lineHeight) * 1.5),
          width: box(table).width,
        }
      })
    })
    // The long cell wraps inside the bubble; nothing runs past it.
    expect(tables[0].inside).toBe(true)
    expect(tables[0].wrapped).toBe(true)
    // The short table stays compact, as it was before cells wrapped: one line per cell, narrower
    // than the table that wraps.
    expect(tables[1].singleLine).toBe(true)
    expect(tables[1].width).toBeLessThan(tables[0].width)
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
