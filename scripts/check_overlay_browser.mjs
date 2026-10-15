// 可选浏览器集成检查：需安装 Playwright，可用环境变量指定模块路径与 Chromium 可执行文件。
// 本地 HTTP 替身使用正式资源表、Worker 与页面，不启动 Tauri 或访问用户的直播配置。
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { readFile, mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { basename, dirname, join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(process.env.PLAYWRIGHT_MODULE_PATH).href : 'playwright')
const assetSource = await readFile(new URL('../src-tauri/src/extensions/overlay_assets.rs', import.meta.url), 'utf8')
const assets = new Map()
for (const [, path, file, mime] of assetSource.matchAll(/asset!\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,?\s*\)/g)) {
  assets.set(`/overlays/${path}`, { mime, bytes: await readFile(new URL(`../public/overlays/${file}`, import.meta.url)) })
}
assert.ok(assets.has('/overlays/shared/connection.js'))
const workerCode = await readFile(new URL('../public/overlays/sw.js', import.meta.url), 'utf8')
const profile = await mkdtemp(join(tmpdir(), 'danmuji-overlay-browser-'))
const ids = ['overtime', 'song-request', 'wish-machine']
let server, context, port = 0, version = 'test-v1', phase = 1, failedDownloads = 0, breakDownload = false
const streams = new Set(), errors = []
const state = id => id === 'wish-machine' ? { goals: [{ id: 'gift', gift_name: '测试心愿', current: phase, target: 10 }] }
  : id === 'song-request' ? { show_username: true, requests: [{ song_name: `测试歌曲${phase}`, username: '测试' }] }
    : { config: { enabled: true, show_rules: false, show_notice: false, rules: [] }, remaining_ms: phase * 60000, running: false, rate: 1, notices: [] }

async function startServer() {
  server = createServer((request, response) => {
    const path = new URL(request.url, 'http://localhost').pathname.replace(/\/index\.html$/, '/')
    response.setHeader('Cache-Control', 'no-store')
    response.setHeader('Content-Security-Policy', "default-src 'self'; script-src 'self'; worker-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; object-src 'none'")
    if (path === '/overlays/sw.js') {
      response.setHeader('Content-Type', 'text/javascript; charset=utf-8')
      response.setHeader('Cache-Control', 'no-cache')
      response.end(`const VERSION = ${JSON.stringify(version)};\nconst ASSETS = ${JSON.stringify([...assets.keys()])};\n${workerCode}`)
      return
    }
    const asset = assets.get(path)
    if (asset) {
      if (breakDownload && path.endsWith('.otf')) { failedDownloads++; response.writeHead(503); response.end(); return }
      response.setHeader('Content-Type', asset.mime)
      response.setHeader('Cache-Control', 'no-cache')
      response.setHeader('X-Danmuji-Overlay-Version', version)
      response.end(path.endsWith('/') ? asset.bytes.toString().replace('</head>', `<meta name="test-version" content="${version}"></head>`) : asset.bytes)
      return
    }
    const id = path.match(/^\/api\/extensions\/([^/]+)\/events$/)?.[1]
    if (ids.includes(id)) {
      response.setHeader('Content-Type', 'text/event-stream')
      response.write(`event: snapshot\ndata: ${JSON.stringify(state(id))}\n\n`)
      streams.add(response)
      request.on('close', () => streams.delete(response))
      return
    }
    response.writeHead(404); response.end()
  })
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(port, '127.0.0.1', resolve) })
  port = server.address().port
}
async function stopServer() {
  if (!server) return
  const closing = new Promise(resolve => server.close(resolve))
  server.closeAllConnections()
  await closing
  server = undefined
}
async function openBrowser() {
  context = await chromium.launchPersistentContext(profile, {
    executablePath: process.env.BROWSER_EXECUTABLE_PATH || undefined,
    headless: true,
  })
  const pages = await Promise.all(ids.map(async id => {
    const page = await context.newPage()
    page.on('pageerror', error => errors.push(String(error)))
    await page.goto(`http://127.0.0.1:${port}/overlays/${id}/`, { waitUntil: 'domcontentloaded' })
    await page.waitForFunction(() => window.OverlayConnection && navigator.serviceWorker.controller)
    return page
  }))
  return pages
}
async function expectState(pages, value) {
  await Promise.all([
    pages[0].waitForFunction(expected => document.getElementById('clock').textContent === `00:0${expected}:00`, value, { timeout: 12000 }),
    pages[1].waitForFunction(expected => document.querySelector('.name')?.textContent === `测试歌曲${expected}`, value, { timeout: 12000 }),
    pages[2].waitForFunction(expected => document.querySelector('.wish-current')?.textContent === String(expected), value, { timeout: 12000 }),
  ])
}
const heartbeat = setInterval(() => { for (const response of streams) response.write('event: heartbeat\ndata:\n\n') }, 10000)
try {
  await startServer()
  let pages = await openBrowser()
  await expectState(pages, 1)
  await context.close(); context = undefined
  await stopServer()
  pages = await openBrowser()
  assert.equal(await pages[0].locator('#clock').innerText(), '连接中…')
  assert.equal(await pages[1].locator('#songs li').count(), 0)
  assert.equal(await pages[2].locator('#wish-list li').count(), 0)
  console.log('通过：完整退出浏览器和服务后，三个页面均能从持久缓存离线冷启动，未缓存直播数据')
  const navigations = [0, 0, 0]
  pages.forEach((page, index) => page.on('framenavigated', frame => { if (frame === page.mainFrame()) navigations[index]++ }))
  phase = 2
  await startServer()
  await expectState(pages, 2)
  assert.deepEqual(navigations, [0, 0, 0])
  console.log('通过：服务稍后启动，三个页面自动恢复完整状态，相同版本无需刷新')
  await stopServer()
  version = 'test-v2'; phase = 3; breakDownload = true
  await startServer()
  await expectState(pages, 3)
  const deadline = Date.now() + 15000
  while (!failedDownloads && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 100))
  assert.ok(failedDownloads > 0)
  assert.deepEqual(navigations, [0, 0, 0])
  assert.equal(await pages[0].locator('meta[name="test-version"]').getAttribute('content'), 'test-v1')
  console.log('通过：新版资源下载失败时保留旧页面，实时数据仍可恢复')
  breakDownload = false
  await Promise.all(pages.map(page => page.waitForFunction(() => document.querySelector('meta[name="test-version"]')?.content === 'test-v2', null, { timeout: 45000 })))
  await expectState(pages, 3)
  assert.deepEqual(navigations, [1, 1, 1])
  assert.deepEqual(await pages[0].evaluate(() => caches.keys()), ['danmuji-overlays-test-v2'])
  assert.equal(await pages[0].evaluate(async () => (await (await caches.open('danmuji-overlays-test-v2')).keys()).some(request => new URL(request.url).pathname.startsWith('/api/'))), false)
  assert.deepEqual(errors, [])
  console.log('通过：周期检查自动补全新版，每个页面只刷新一次，旧缓存已清理且 API 未落盘')
} finally {
  clearInterval(heartbeat)
  await context?.close()
  await stopServer()
  assert.equal(dirname(resolve(profile)), resolve(tmpdir()))
  assert.ok(basename(profile).startsWith('danmuji-overlay-browser-'))
  await rm(profile, { recursive: true, force: true })
}
