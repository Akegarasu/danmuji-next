(() => {
  const songs = document.getElementById('songs')
  let lastSnapshot = ''
  function render(state) {
    if (!state || !Array.isArray(state.requests)) return
    const signature = JSON.stringify(state)
    if (signature === lastSnapshot) return
    lastSnapshot = signature
    window.OverlayStyle.apply(state.overlay_style)
    const rows = state.requests.map((song, index) => {
      const row = document.createElement('li')
      row.className = 'song'
      for (const [className, value] of [['number', String(index + 1).padStart(2, '0')], ['name', song.song_name], ...(state.show_username ? [['username', song.username]] : [])]) {
        const cell = document.createElement('span')
        cell.className = className
        // 观众输入始终按纯文本显示。
        cell.textContent = value
        row.appendChild(cell)
      }
      return row
    })
    songs.replaceChildren(...rows)
  }
  const source = new EventSource('/api/extensions/song-request/events')
  source.addEventListener('snapshot', event => {
    try { render(JSON.parse(event.data)) }
    catch { /* 保留上一份有效队列，避免向直播画面显示诊断信息。 */ }
  })
  source.onerror = () => { /* 静默自动重连，覆盖层只展示歌曲。 */ }
  // EventSource 自动重连，服务端在每次连接时发送完整队列。
  window.addEventListener('pagehide', () => source.close())
})()
