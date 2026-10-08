(() => {
  const list = document.getElementById('wish-list')
  let lastSnapshot = ''
  function render(state) {
    if (!state || !Array.isArray(state.goals) || !state.goals.every(goal =>
      typeof goal.id === 'string' && /^[a-zA-Z0-9_-]+$/.test(goal.id) && typeof goal.gift_name === 'string' &&
      Number.isSafeInteger(goal.current) && goal.current >= 0 && Number.isSafeInteger(goal.target) && goal.target > 0)) return
    const signature = JSON.stringify(state.goals)
    if (signature === lastSnapshot) return
    const rows = state.goals.map(goal => {
      const row = document.createElement('li')
      row.id = `wish-goal-${goal.id}`
      row.className = 'wish-row'
      row.dataset.complete = String(goal.current >= goal.target)
      const name = document.createElement('span')
      name.id = `wish-name-${goal.id}`
      name.className = 'wish-name'
      name.textContent = goal.gift_name
      const count = document.createElement('span')
      count.id = `wish-count-${goal.id}`
      count.className = 'wish-count'
      for (const [part, value] of [['current', goal.current], ['separator', '/'], ['target', goal.target]]) {
        const cell = document.createElement('span')
        cell.id = `wish-${part}-${goal.id}`
        cell.className = `wish-${part}`
        cell.textContent = String(value)
        count.appendChild(cell)
      }
      row.appendChild(name)
      row.appendChild(count)
      return row
    })
    list.replaceChildren(...rows)
    lastSnapshot = signature
  }
  const source = new EventSource('/api/extensions/wish-machine/events')
  source.addEventListener('snapshot', event => {
    try { render(JSON.parse(event.data)) }
    catch { /* 保留最后的有效心愿，不将诊断信息显示到直播中。 */ }
  })
  source.onerror = () => { /* 自动重连后会收到完整快照。 */ }
  window.addEventListener('pagehide', () => source.close())
})()
