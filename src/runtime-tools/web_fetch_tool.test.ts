// @vitest-environment node

import { Type } from '@sinclair/typebox'
import { access, mkdir, mkdtemp, readFile, stat, writeFile } from 'node:fs/promises'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import {
  buildStructuredFetchResponse,
  createWebFetchTool,
  isWeChatArticleUrl,
  normalizeWebFetchInput,
  parseWeChatArticleHtml,
  resolveFetchUserAgent,
} from './web_fetch_tool.mjs'

function createDeps(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    Type,
    execFileImpl: vi.fn(async () => ({
      stdout: '__NC_CURL_META__200\ttext/html; charset=utf-8\thttps://example.com/final',
      stderr: '',
    })),
    fsPromises: {
      mkdir,
      access,
      writeFile,
      readFile,
    },
    fsConstants: fs.constants,
    fsSync: fs,
    pathApi: path,
    osApi: os,
    cryptoApi: { randomBytes: (size: number) => Buffer.alloc(size, 9) },
    processApi: { once: vi.fn(), env: {} },
    withFileMutationQueue: async (_target: string, fn: () => Promise<unknown>) => fn(),
    ...overrides,
  }
}

describe('web_fetch helpers', () => {
  it('normalizes input and detects WeChat URLs', () => {
    const normalized = normalizeWebFetchInput({
      url: ' https://mp.weixin.qq.com/s/abc ',
      ua: ' custom-agent ',
    })

    expect(normalized).toEqual({
      url: 'https://mp.weixin.qq.com/s/abc',
      ua: 'custom-agent',
    })
    expect(isWeChatArticleUrl(normalized.url)).toBe(true)
  })

  it('uses WeChat UA by default for WeChat articles', () => {
    const ua = resolveFetchUserAgent(
      normalizeWebFetchInput({ url: 'https://mp.weixin.qq.com/s/abc' }),
    )

    expect(ua).toContain('MicroMessenger')
  })

  it('parses WeChat article content into a consistent structure', () => {
    const html = `
      <meta property="og:title" content="公众号标题" />
      <meta name="description" content="文章摘要" />
      <script>var x = 1;</script>
      nick_name: JsDecode('NineClaw')
      <div id="js_content">
        <p>第一段</p>
        <p>第二段</p>
        <img data-src="https://mmbiz.qpic.cn/abc" />
      </div></div></div>
    `

    const article = parseWeChatArticleHtml(html, 'https://mp.weixin.qq.com/s/abc')
    expect(article).toMatchObject({
      title: '公众号标题',
      author: 'NineClaw',
      description: '文章摘要',
      isVideo: false,
    })
    expect(article.contentText).toContain('第一段')
    expect(article.images).toEqual(['https://mmbiz.qpic.cn/abc'])
  })

  it('builds structured JSON responses for HTML pages', () => {
    const response = buildStructuredFetchResponse(
      normalizeWebFetchInput({ url: 'https://example.com/post' }),
      {
        status: 200,
        contentType: 'text/html; charset=utf-8',
        finalUrl: 'https://example.com/post',
        headersText: 'HTTP/2 200\ncontent-type: text/html; charset=utf-8\n',
        bodyBuffer: Buffer.from(`
          <html>
            <head>
              <title>Hello world</title>
              <meta name="description" content="Short desc" />
              <meta name="author" content="Ada" />
            </head>
            <body>
              <article>
                <p>Alpha</p>
                <p>Beta</p>
                <a href="/docs">Docs</a>
                <img src="/hero.png" />
              </article>
            </body>
          </html>
        `),
      },
      createDeps({ processApi: { once: vi.fn(), env: { HTTPS_PROXY: 'http://127.0.0.1:7890' } } }) as never,
    )

    expect(response).toMatchObject({
      ok: true,
      kind: 'html',
      title: 'Hello world',
      author: 'Ada',
      description: 'Short desc',
      finalUrl: 'https://example.com/post',
    })
    expect(response.contentText).toContain('Alpha')
    expect(response.images).toEqual(['https://example.com/hero.png'])
    expect(response.links[0]).toEqual({
      url: 'https://example.com/docs',
      text: 'Docs',
    })
    expect(response.proxy).toEqual({
      mode: 'environment_proxy',
      envKeys: ['HTTPS_PROXY'],
    })
  })
})

describe('web_fetch tool execute', () => {
  it('uses the provided UA and spills large structured results to a file', async () => {
    const cwd = await mkdtemp(path.join(os.tmpdir(), 'web-fetch-exec-'))
    const bodyText = `<html><body><article><p>${'fetch result '.repeat(1500)}</p></article></body></html>`
    const bodyBuffer = Buffer.from(bodyText)
    const deps = createDeps({
      execFileImpl: vi.fn(async (_command: string, args: string[]) => {
        const headerPath = args[args.indexOf('-D') + 1]
        const bodyPath = args[args.indexOf('-o') + 1]
        await writeFile(headerPath, 'HTTP/2 200\ncontent-type: text/html; charset=utf-8\n')
        await writeFile(bodyPath, bodyBuffer)
        return {
          stdout: '__NC_CURL_META__200\ttext/html; charset=utf-8\thttps://example.com/final',
          stderr: '',
        }
      }),
    })

    const tool = createWebFetchTool(deps as never)
    const result = await tool.execute(
      'tool-call-1',
      {
        url: 'https://example.com/raw',
        ua: 'custom-agent/1.0',
      },
      undefined,
      undefined,
      { cwd },
    )

    expect(result.content[0].text).toContain('Result exceeded 12000 characters and was written to')
    const filePath = result.details.storage.filePath as string
    expect(await stat(filePath)).toBeTruthy()
    const stored = await readFile(filePath, 'utf8')
    expect(stored).toContain('"url": "https://example.com/raw"')
    expect(stored).toContain('"effectiveUa": "custom-agent/1.0"')
    expect(stored).toContain('"kind": "html"')
  })

  it('returns a stable error payload for invalid URLs', async () => {
    const tool = createWebFetchTool(createDeps() as never)
    const result = await tool.execute(
      'tool-call-2',
      {
        url: 'not-a-url',
      },
      undefined,
      undefined,
      { cwd: process.cwd() },
    )

    const payload = JSON.parse(result.content[0].text)
    expect(payload).toMatchObject({
      url: 'not-a-url',
      ua: null,
      ok: false,
      kind: 'error',
      error: 'url must be an absolute http(s) URL',
    })
  })
})
