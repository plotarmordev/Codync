// Seeds a roster large enough to exercise lazy row creation in both scroll directions.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (typeof CLEANUP !== 'undefined') {
  for (const bot of output.bots) api('deleteBot', { botId: bot.id })
} else {
  output.bots = []
  for (let n = 1; n <= 60; n++) {
    const bot = api('createBot', {
      name: `Roster load ${n}`, backend: 'custom', command: `python3 -X utf8 -u '${HISTORY_AGENT}'`,
      connectors: [], skills: [], notify: false,
    }).bot
    output.bots.push({ id: bot.id })
  }
}
