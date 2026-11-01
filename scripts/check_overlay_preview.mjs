// 使用真实 Vue 组件检查 Vite 开发模式和生产构建的预览字体、样式与 CSS 编辑。
// 与 check_overlay_browser.mjs 相同，可通过环境变量指定 Playwright 和浏览器。
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build, createServer, preview } from 'vite'

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(process.env.PLAYWRIGHT_MODULE_PATH).href : 'playwright')
const root = fileURLToPath(new URL('../', import.meta.url))
const testsRoot = join(root, '.tests')
await mkdir(testsRoot, { recursive: true })
const fixture = await mkdtemp(join(testsRoot, 'overlay-preview-'))
const input = join(fixture, 'index.html')
const outDir = join(fixture, 'dist')
const route = '/' + relative(root, input).replaceAll('\\', '/')
const fontPath = '/overlays/overtime/timer-heavy.otf'
const digest = bytes => createHash('sha256').update(bytes).digest('hex')
const fontDigest = digest(await readFile(join(root, 'public', fontPath)))
await writeFile(input, `<!doctype html><html><head><meta charset="utf-8"></head><body>
<div id="app"></div><script type="module">
import { createApp, h, reactive } from 'vue'
import WishStyleSettings from '/src/components/extension/WishStyleSettings.vue'
const config = reactive({ theme: 'test-a', theme_a_font_family: '', custom_css: '' })
const goals = [{ id: 'gift', gift_name: '测试心愿 <b>文字</b>', current: 2, target: 5 }]
createApp({ render: () => h(WishStyleSettings, {
  config, goals, disabled: false, onChange: value => Object.assign(config, value),
}) }).mount('#app')
</script></body></html>`, 'utf8')
let server, browser
try {
  browser = await chromium.launch({ executablePath: process.env.BROWSER_EXECUTABLE_PATH || undefined, headless: true })
  for (const mode of ['dev', 'build']) {
    if (mode === 'dev') {
      server = await createServer({ root, server: { host: '127.0.0.1', port: 0, strictPort: false } })
      await server.listen()
    } else {
      await build({ root, build: { outDir, rollupOptions: { input }, emptyOutDir: false } })
      server = await preview({ root, build: { outDir }, preview: { host: '127.0.0.1', port: 0, strictPort: false } })
    }
    const origin = `http://127.0.0.1:${server.httpServer.address().port}`
    const page = await browser.newPage()
    const errors = []
    page.on('pageerror', error => errors.push(String(error)))
    let failTheme = true
    await page.route('**/overlays/wish-machine/theme-test-a.css', async route => {
      if (failTheme) { failTheme = false; await route.fulfill({ status: 503, body: 'unavailable' }) }
      else await route.continue()
    })
    await page.goto(origin + route)
    await page.getByText('主题样式加载失败，请重试。').waitFor()
    assert.equal(await page.getByRole('button', { name: '复制 CSS' }).isDisabled(), true)
    await page.getByRole('button', { name: '重试', exact: true }).click()
    await page.waitForFunction(() => document.querySelector('textarea')?.value.includes('#6978ff'))
    const frame = page.frameLocator('iframe[title="心愿机主题预览"]')
    await frame.locator('.wish-name').waitFor()
    assert.equal(await frame.locator('.wish-name').innerText(), '测试心愿 <b>文字</b>')
    assert.equal(await frame.locator('.wish-name b').count(), 0)
    await frame.locator('.wish-row').evaluate(async element => {
      await document.fonts.ready
      if (![...document.fonts].some(font => font.family === 'WishHeavy' && font.status === 'loaded')) {
        throw new Error('内置字体未成功加载')
      }
      if (getComputedStyle(element).backgroundColor !== 'rgb(105, 120, 255)') throw new Error('主题样式未生效')
    })
    const response = await page.request.get(origin + fontPath)
    assert.equal(response.status(), 200)
    assert.equal(digest(await response.body()), fontDigest)
    // 基础样式必须独立加载；自定义 CSS 仍可即时覆盖，且不影响同源字体。
    assert.equal(await frame.locator('link[rel="stylesheet"]').getAttribute('href'), '/overlays/wish-machine/overlay.css')
    await page.getByRole('textbox', { name: 'OBS 自定义 CSS' }).fill('#wish-machine { font-size: 37px; }')
    await page.waitForFunction(() => {
      const doc = document.querySelector('iframe')?.contentDocument
      return doc?.getElementById('wish-machine') && getComputedStyle(doc.getElementById('wish-machine')).fontSize === '37px'
    })
    assert.deepEqual(errors, [])
    console.log(`通过：${mode} 模式应用内预览、中文字体、主题读取失败重试和自定义 CSS`)
    await page.close()
    if (mode === 'dev') await server.close()
    else await new Promise(resolve => server.httpServer.close(resolve))
    server = undefined
  }
} finally {
  await browser?.close()
  if (server?.close) await server.close()
  else if (server) await new Promise(resolve => server.httpServer.close(resolve))
  assert.equal(dirname(resolve(fixture)), resolve(testsRoot))
  await rm(fixture, { recursive: true, force: true })
}
