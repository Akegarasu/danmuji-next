/* OBS 独立入口：不加载 Vue 主应用、Tauri、B 站连接或第三方接口。 */
(() => {
  const room = document.querySelector('.timer-room');
  const resize = () => { room.style.zoom = Math.min(1, window.innerWidth / 600); };
  resize();
  window.addEventListener('resize', resize);
  const clock = document.getElementById('clock');
  const connection = document.getElementById('connection');
  const rules = document.getElementById('rules');
  const noticeText = document.getElementById('notice-text');
  let snapshot = null;
  let receivedAt = 0;
  let connected = false;
  let lastNotice = null;
  let configKey = '';
  let queue = [];
  let showingNotice = false;
  let noticeTimeout;

  const duration = seconds => {
    if (seconds >= 3600 && seconds % 3600 === 0) return `${seconds / 3600}时`;
    if (seconds >= 60 && seconds % 60 === 0) return `${seconds / 60}分`;
    return `${seconds}秒`;
  };
  const actionLabel = rule => {
    switch (rule.action) {
      case 'add': return `+${duration(rule.value)}`;
      case 'subtract': return `-${duration(rule.value)}`;
      case 'multiply': return `×${rule.value}`;
      case 'divide': return `÷${rule.value}`;
      case 'set_time': return `设为${duration(rule.value)}`;
      case 'set_rate': return `流速×${rule.value}`;
      case 'clear': return '清空';
      case 'random': return '随机';
      default: return '';
    }
  };
  function clearNotice() {
    queue = [];
    showingNotice = false;
    clearTimeout(noticeTimeout);
    noticeText.textContent = '';
    noticeText.classList.remove('scrolling');
    lastNotice = null;
  }
  function showNextNotice() {
    if (showingNotice || queue.length === 0) return;
    showingNotice = true;
    const notice = queue.shift();
    let effect = `${notice.delta_ms < 0 ? '-' : '+'}${duration(Math.round(Math.abs(notice.delta_ms) / 1000))}`;
    if (notice.actions.includes('clear')) effect = '清空时间';
    else if (notice.actions.includes('set_rate') && notice.delta_ms === 0) effect = '调整倒计时速度';
    const draws = (notice.results || []).filter(result => result.random).map(result => `随机${actionLabel(result)}`);
    if (draws.length) effect = `${draws.join('、')}（${effect}）`;
    noticeText.textContent = `${notice.sender_name} 投喂 ${notice.gift_name}×${notice.num} ${effect}`;
    noticeText.classList.remove('scrolling');
    void noticeText.offsetWidth;
    noticeText.classList.add('scrolling');
    // 与参考页一致：展示 10 秒后，间隔 1.5 秒取下一条。
    // 队列为空时保留最后一条，由 CSS 无限循环；循环只影响显示。
    noticeTimeout = setTimeout(() => {
      showingNotice = false;
      showNextNotice();
    }, 11500);
  }
  function update(value) {
    snapshot = value;
    receivedAt = performance.now();
    connected = true;
    const nextKey = JSON.stringify(value.config);
    if (nextKey !== configKey) {
      configKey = nextKey;
      rules.replaceChildren();
      if (value.config.show_rules) {
        for (const rule of value.config.rules.filter(rule => rule.enabled)) {
          const item = document.createElement('div');
          item.className = 'rule';
          const name = document.createElement('span');
          name.className = 'rule-name';
          name.textContent = `·${rule.gift_name}`;
          const action = document.createElement('span');
          action.className = 'rule-action';
          action.textContent = actionLabel(rule);
          item.append(name, action);
          if (rule.action === 'random') {
            const ranges = document.createElement('small');
            ranges.textContent = (rule.random_ranges || []).map(range => {
              const symbol = { add: '+', subtract: '-', multiply: '×', divide: '÷' }[range.action];
              return `${symbol}${range.min}～${range.max}${['add', 'subtract'].includes(range.action) ? '秒' : ''}`;
            }).join(' / ');
            item.append(ranges);
          }
          if (!rule.per_gift) {
            const note = document.createElement('small'); note.textContent = '每条通知触发一次'; item.append(note);
          }
          rules.append(item);
        }
      }
    }
    const notices = value.notices || [];
    const newest = notices.length ? notices[notices.length - 1].id : 0;
    if (!value.config.show_notice || !value.config.enabled || notices.length === 0) {
      clearNotice();
    } else if (lastNotice === null || newest < lastNotice) {
      // 首帧/重连只恢复最新提示，不逐条重放历史；计时结果始终来自服务端。
      clearNotice();
      queue.push(notices[notices.length - 1]);
      lastNotice = newest;
      showNextNotice();
    } else {
      queue.push(...notices.filter(notice => notice.id > lastNotice));
      queue = queue.slice(-20);
      lastNotice = newest;
      showNextNotice();
    }
    render();
  }
  function render() {
    if (!snapshot) return;
    const stale = !connected || performance.now() - receivedAt > 4000;
    // 断线即冻结上次权威值，避免误显示“下班啦”。恢复后使用服务端快照校准。
    const elapsed = !stale && snapshot.running && snapshot.config.enabled ? (performance.now() - receivedAt) * snapshot.rate : 0;
    const seconds = Math.ceil(Math.max(0, snapshot.remaining_ms - elapsed) / 1000);
    const hours = Math.floor(seconds / 3600);
    const display = `${String(hours).padStart(2, '0')}:${String(Math.floor(seconds / 60) % 60).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
    clock.textContent = seconds === 0 ? '下班啦！' : display;
    clock.classList.toggle('long', hours > 99);
    connection.textContent = stale ? '连接已断开，正在重连…' : !snapshot.config.enabled ? '加班机未启用' : !snapshot.running ? '已暂停' : snapshot.rate !== 1 ? `倒计时速度 ×${snapshot.rate}` : '';
  }
  const source = new EventSource('/api/extensions/overtime/events');
  source.addEventListener('snapshot', event => {
    try { update(JSON.parse(event.data)); }
    catch (error) { console.error('加班机状态解析失败', error); }
  });
  source.onerror = () => {
    connected = false; lastNotice = null; queue = []; showingNotice = false;
    clearTimeout(noticeTimeout); render();
  };
  const renderInterval = setInterval(render, 100);
  window.addEventListener('pagehide', () => {
    source.close(); clearInterval(renderInterval); clearTimeout(noticeTimeout); window.removeEventListener('resize', resize);
  }, { once: true });
})();
