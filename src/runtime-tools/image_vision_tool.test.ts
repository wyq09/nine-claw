// @vitest-environment node

import { Type } from '@sinclair/typebox'

import {
  createImageVisionTool,
  normalizeImageVisionInput,
  toVisionSources,
} from './image_vision_tool.mjs'

function createDeps(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    Type,
    fetchImpl: vi.fn(async () => ({
      ok: true,
      status: 200,
      text: async () =>
        JSON.stringify({
          text: '图中是一张产品截图，标题写着 NineClaw。',
          model: 'vision-model-x',
          apiFormat: 'openai',
          imageCount: 2,
          notes: ['已从视频 /tmp/a.mp4 提取 3 帧关键画面'],
        }),
    })),
    processApi: {
      env: {
        NINECLAW_PROXY_BASE_URL: 'http://127.0.0.1:8123',
        NINECLAW_PROXY_SESSION_TOKEN: 'token-1',
      },
      cwd: () => process.cwd(),
    },
    ...overrides,
  }
}

describe('image_vision tool helpers', () => {
  it('normalizes vision input and caps the image list', () => {
    expect(
      normalizeImageVisionInput({
        images: ['  /tmp/a.png ', '', null, 'https://example.com/b.jpg'],
        question: '  图里有什么文字？  ',
      }),
    ).toEqual({
      images: ['/tmp/a.png', 'https://example.com/b.jpg'],
      question: '图里有什么文字？',
    })

    const tooMany = normalizeImageVisionInput({
      images: Array.from({ length: 9 }, (_, index) => `/tmp/${index}.png`),
      question: undefined,
    })
    expect(tooMany.images).toHaveLength(6)
    expect(tooMany.images[5]).toBe('/tmp/5.png')
    expect(tooMany.question).toBeUndefined()
  })

  it('splits sources into local paths and remote urls', () => {
    expect(
      toVisionSources(['/tmp/img.png', 'https://example.com/x.jpg', 'http://example.com/y.webp']),
    ).toEqual([
      { path: '/tmp/img.png' },
      { url: 'https://example.com/x.jpg' },
      { url: 'http://example.com/y.webp' },
    ])
  })
})

describe('image_analyze tool execute', () => {
  it('posts sources to the vision gateway and returns the description', async () => {
    const fetchImpl = vi.fn(async () => ({
      ok: true,
      status: 200,
      text: async () =>
        JSON.stringify({
          text: '截图里是一个登录表单。',
          model: 'vision-model-x',
          apiFormat: 'anthropic',
          imageCount: 1,
          notes: [],
        }),
    }))
    const tool = createImageVisionTool(createDeps({ fetchImpl }) as never)

    const updates: unknown[] = []
    const result = await tool.execute(
      'tool-call-1',
      { images: ['/tmp/login.png'], question: '这个界面有什么问题？' },
      undefined,
      (update: unknown) => updates.push(update),
      { cwd: process.cwd() },
    )

    expect(fetchImpl).toHaveBeenCalledWith(
      'http://127.0.0.1:8123/vision/token-1/describe',
      {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          images: [{ path: '/tmp/login.png' }],
          prompt: '这个界面有什么问题？',
        }),
        signal: undefined,
      },
    )
    expect(result.content[0].text).toContain('[image_analyze] model=vision-model-x images=1')
    expect(result.content[0].text).toContain('截图里是一个登录表单。')
    expect(result.details).toMatchObject({ ok: true, model: 'vision-model-x', imageCount: 1 })
    expect(updates).toHaveLength(1)
  })

  it('returns a stable error result when no images are provided', async () => {
    const tool = createImageVisionTool(createDeps() as never)
    const result = await tool.execute(
      'tool-call-2',
      { images: [] },
      undefined,
      undefined,
      { cwd: process.cwd() },
    )
    expect(result.content[0].text).toContain('No image paths or URLs were provided')
    expect(result.details).toMatchObject({ ok: false, error: 'images is required' })
  })

  it('returns a stable error result when the gateway env is unavailable', async () => {
    const tool = createImageVisionTool(
      createDeps({ processApi: { env: {}, cwd: () => process.cwd() } }) as never,
    )
    const result = await tool.execute(
      'tool-call-3',
      { images: ['/tmp/a.png'] },
      undefined,
      undefined,
      { cwd: process.cwd() },
    )
    expect(result.content[0].text).toContain('Vision gateway is not available')
    expect(result.details).toMatchObject({ ok: false, error: 'missing vision gateway env' })
  })

  it('explains next steps when the vision model is not configured', async () => {
    const fetchImpl = vi.fn(async () => ({
      ok: false,
      status: 400,
      text: async () => 'no image vision runtime configured',
    }))
    const tool = createImageVisionTool(createDeps({ fetchImpl }) as never)
    const result = await tool.execute(
      'tool-call-4',
      { images: ['/tmp/a.png'] },
      undefined,
      undefined,
      { cwd: process.cwd() },
    )
    expect(result.content[0].text).toContain('system vision model is not configured')
    expect(result.details).toMatchObject({ ok: false, error: 'vision model not configured' })
  })

  it('throws with the gateway error body on upstream failures', async () => {
    const fetchImpl = vi.fn(async () => ({
      ok: false,
      status: 502,
      text: async () => 'upstream vision model timeout',
    }))
    const tool = createImageVisionTool(createDeps({ fetchImpl }) as never)
    await expect(
      tool.execute('tool-call-5', { images: ['/tmp/a.png'] }, undefined, undefined, {
        cwd: process.cwd(),
      }),
    ).rejects.toThrow('upstream vision model timeout')
  })
})