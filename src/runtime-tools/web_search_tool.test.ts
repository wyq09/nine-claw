// @vitest-environment node

import { Type } from '@sinclair/typebox'
import { access, mkdir, mkdtemp, readFile, stat, writeFile } from 'node:fs/promises'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import {
  buildEngineUrl,
  composeSearchQuery,
  createWebSearchTool,
  extractSearchResults,
  normalizeWebSearchInput,
  SEARCH_ENGINES,
} from './web_search_tool.mjs'
import { finalizeLargeTextResult } from './tool_result_storage.mjs'

function createDeps(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    Type,
    fetchImpl: async () => {
      throw new Error('fetch not mocked')
    },
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
    cryptoApi: { randomBytes: (size: number) => Buffer.alloc(size, 7) },
    processApi: { once: vi.fn() },
    withFileMutationQueue: async (_target: string, fn: () => Promise<unknown>) => fn(),
    ...overrides,
  }
}

describe('web_search tool helpers', () => {
  it('composes advanced operator queries', () => {
    const normalized = normalizeWebSearchInput({
      query: 'rust async',
      site: 'docs.rs',
      fileType: 'pdf',
      exactTerms: ['tokio tutorial'],
      excludeTerms: ['old'],
      orTerms: ['sqlx', 'sea-orm'],
    })

    expect(composeSearchQuery(normalized)).toBe(
      'rust async site:docs.rs filetype:pdf "tokio tutorial" -old (sqlx OR sea-orm)',
    )
  })

  it('adds engine specific search parameters to URLs', () => {
    const google = SEARCH_ENGINES.find((engine) => engine.key === 'google')
    expect(google).toBeTruthy()

    const normalized = normalizeWebSearchInput({
      query: 'ai news',
      timeRange: 'past_week',
      searchType: 'news',
      language: 'en-US',
    })

    const url = buildEngineUrl(google!, normalized)
    expect(url).toContain('q=ai+news')
    expect(url).toContain('tbs=qdr%3Aw')
    expect(url).toContain('tbm=nws')
    expect(url).toContain('hl=en-US')
  })

  it('extracts results from duckduckgo html', () => {
    const engine = SEARCH_ENGINES.find((item) => item.key === 'duckduckgo')
    expect(engine).toBeTruthy()

    const html = `
      <div class="result">
        <a class="result__a" href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.rs%2Fsqlx">sqlx - Rust SQL toolkit</a>
        <div class="result__snippet">Docs and examples for SQLx.</div>
      </div>
    `

    const results = extractSearchResults(engine!, html, 3)
    expect(results).toHaveLength(1)
    expect(results[0]).toMatchObject({
      title: 'sqlx - Rust SQL toolkit',
      url: 'https://docs.rs/sqlx',
      snippet: 'Docs and examples for SQLx.',
    })
  })

  it('extracts results from modern sogou result blocks', () => {
    const engine = SEARCH_ENGINES.find((item) => item.key === 'sogou')
    expect(engine).toBeTruthy()

    const html = `
      <!-- ResultListViewBegin -->
      <div class="vrwrap" id="sogou_vr_30000000_wrap_4">
        <h3 class="vr-title"><a href="/link?url=abc" id="sogou_vr_30000000_4">18只偏股型基金近3月收益率超30%</a></h3>
        <div class="fz-mid space-txt base-ellipsis clamp2" id="cacheresult_summary_4">其中18只基金收益率超过30%，最高超过44%。</div>
        <div class="r-sech ext_query" data-url="http://finance.example.com/story"></div>
      </div><!--STATUS VR OK-->
      <div class="vrwrap" id="sogou_vr_30000000_wrap_6">
        <h3 class="vr-title"><a href="/link?url=def" id="sogou_vr_30000000_6">最高收益率近40%！公募新年首月惊喜开局</a></h3>
        <div class="fz-mid space-txt base-ellipsis clamp2" id="cacheresult_summary_6">10只基金产品在2025年第一月取得超20%的高收益。</div>
        <div class="r-sech ext_query" data-url="https://finance.example.com/second"></div>
      </div><!--STATUS VR OK-->
      <!-- HintViewBegin -->
    `

    const results = extractSearchResults(engine!, html, 3)
    expect(results).toEqual([
      {
        title: '18只偏股型基金近3月收益率超30%',
        url: 'http://finance.example.com/story',
        snippet: '其中18只基金收益率超过30%，最高超过44%。',
      },
      {
        title: '最高收益率近40%！公募新年首月惊喜开局',
        url: 'https://finance.example.com/second',
        snippet: '10只基金产品在2025年第一月取得超20%的高收益。',
      },
    ])
  })

  it('writes oversized results into the workdir result folder', async () => {
    const cwd = await mkdtemp(path.join(os.tmpdir(), 'web-search-workdir-'))
    const deps = createDeps()

    const finalized = await finalizeLargeTextResult(
      'x'.repeat(120),
      { cwd },
      {
        filePrefix: 'web-search',
        maxResultSizeChars: 40,
        tempArtifactTracker: { register: vi.fn() },
      },
      deps,
    )

    expect(finalized.storage.inline).toBe(false)
    const filePath = finalized.storage.filePath as string
    expect(filePath.startsWith(path.join(cwd, '.nineclaw-tool-results'))).toBe(true)
    expect((await readFile(filePath, 'utf8')).length).toBe(120)
  })
})

describe('web_search tool execute', () => {
  it('renders parsed results and spills large outputs', async () => {
    const cwd = await mkdtemp(path.join(os.tmpdir(), 'web-search-exec-'))
    const deps = createDeps({
      fetchImpl: vi.fn(async (url: string) => {
        if (url.includes('duckduckgo.com')) {
          return {
            ok: true,
            status: 200,
            text: async () => `
              <div class="result">
                <a class="result__a" href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fdoc">Example result</a>
                <div class="result__snippet">${'fresh result '.repeat(240)}</div>
              </div>
            `,
          }
        }
        return {
          ok: true,
          status: 200,
          text: async () => `
            <li class="b_algo">
              <h2><a href="https://example.org/page">Bing Result</a></h2>
              <div class="b_caption"><p>${'Useful snippet '.repeat(200)}</p></div>
            </li>
          `,
        }
      }),
    })

    const tool = createWebSearchTool(deps as never)
    const result = await tool.execute(
      'tool-call-1',
      {
        query: 'fresh docs',
        engines: ['duckduckgo', 'bing_int'],
        maxResultSizeChars: 1000,
      },
      undefined,
      undefined,
      { cwd },
    )

    const text = result.content[0].text
    expect(text).toContain('Result exceeded 1000 characters and was written to')
    expect(result.details.storage?.filePath).toBeTruthy()
    const filePath = result.details.storage?.filePath
    if (!filePath) {
      throw new Error('expected stored web search payload path')
    }
    expect(filePath).toBeTruthy()
    expect(await stat(filePath)).toBeTruthy()
    const stored = await readFile(filePath, 'utf8')
    expect(stored).toContain('[DuckDuckGo]')
    expect(stored).toContain('Example result')
    expect(stored).toContain('[Bing INT]')
    expect(result.details.storage?.inline).toBe(false)
  })

  it('downloads images discovered on search result pages', async () => {
    const cwd = await mkdtemp(path.join(os.tmpdir(), 'web-search-images-'))
    const deps = createDeps({
      fetchImpl: vi.fn(async (url: string) => {
        if (url === 'https://example.com/thumb.webp') {
          return {
            ok: true,
            status: 200,
            url,
            headers: { get: (name: string) => (name.toLowerCase() === 'content-type' ? 'image/webp' : null) },
            arrayBuffer: async () => new Uint8Array([5, 6, 7]).buffer,
          }
        }
        return {
          ok: true,
          status: 200,
          text: async () => `
            <div class="result">
              <a class="result__a" href="https://example.com/story">Example story</a>
              <div class="result__snippet">Useful story.</div>
              <img src="https://example.com/thumb.webp" />
            </div>
          `,
        }
      }),
    })

    const tool = createWebSearchTool(deps as never)
    const result = await tool.execute(
      'tool-call-images',
      {
        query: 'image story',
        engines: ['duckduckgo'],
        maxImages: 1,
      },
      undefined,
      undefined,
      { cwd },
    )

    expect(result.content[0].text).toContain('Downloaded images:')
    expect(result.details.engines[0].images).toEqual(['https://example.com/thumb.webp'])
    expect(result.details.engines[0].downloadedImages[0]).toMatchObject({
      url: 'https://example.com/thumb.webp',
      ok: true,
      contentType: 'image/webp',
      bytes: 3,
    })
    const imagePath = result.details.engines[0].downloadedImages[0].filePath as string
    expect(imagePath.startsWith(path.join(cwd, '.nineclaw-tool-results', 'images'))).toBe(true)
    expect(await readFile(imagePath)).toEqual(Buffer.from([5, 6, 7]))
  })

  it('reports google anti-bot interstitials explicitly when using curl transport', async () => {
    const cwd = await mkdtemp(path.join(os.tmpdir(), 'web-search-google-'))
    const deps = createDeps({
      execFileImpl: vi.fn(async (_command: string, args: string[]) => {
        const headerPath = args[args.indexOf('-D') + 1]
        const bodyPath = args[args.indexOf('-o') + 1]
        await writeFile(headerPath, 'HTTP/2 200\ncontent-type: text/html; charset=utf-8\n')
        await writeFile(
          bodyPath,
          '<!DOCTYPE html><html><head><title>Google Search</title></head><body><meta content="0;url=/httpservice/retry/enablejs?sei=abc" http-equiv="refresh"></body></html>',
        )
        return {
          stdout: '__NC_CURL_META__200\ttext/html; charset=utf-8\thttps://www.google.com/search?q=test',
          stderr: '',
        }
      }),
    })

    const tool = createWebSearchTool(deps as never)
    const result = await tool.execute(
      'tool-call-2',
      {
        query: 'fresh docs',
        engines: ['google'],
      },
      undefined,
      undefined,
      { cwd },
    )

    expect(result.content[0].text).toContain('Google returned an anti-bot / enablejs interstitial')
    expect(result.details.engines[0]).toMatchObject({
      transport: 'curl',
      error: 'Google returned an anti-bot / enablejs interstitial instead of search results.',
    })
  })
})
