// @vitest-environment node

import { Type } from '@sinclair/typebox'
import { mkdir, mkdtemp, readFile } from 'node:fs/promises'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import {
  createImageGenerationTool,
  normalizeImageGenerationInput,
} from './image_generation_tool.mjs'

function createDeps(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    Type,
    fetchImpl: vi.fn(async () => ({
      ok: true,
      status: 200,
      text: async () =>
        JSON.stringify({
          providerId: 'openai_image',
          adapterType: 'openai_images',
          model: 'gpt-image-1',
          taskId: 'task_local_1',
          images: [
            {
              mimeType: 'image/png',
              dataBase64: Buffer.from('fake-image').toString('base64'),
            },
          ],
        }),
    })),
    fsPromises: { mkdir, writeFile: fs.promises.writeFile },
    pathApi: path,
    osApi: os,
    cryptoApi: { randomBytes: (size: number) => Buffer.alloc(size, 4) },
    processApi: {
      env: {
        NINECLAW_PROXY_BASE_URL: 'http://127.0.0.1:8123',
        NINECLAW_PROXY_SESSION_TOKEN: 'token-1',
      },
      cwd: () => process.cwd(),
    },
    withFileMutationQueue: async (_target: string, fn: () => Promise<unknown>) => fn(),
    ...overrides,
  }
}

describe('image_generation tool helpers', () => {
  it('normalizes image generation input', () => {
    expect(
      normalizeImageGenerationInput({
        prompt: '  poster  ',
        size: ' 1536x1024 ',
        resolution: '2K',
        count: 9,
        outputFormat: 'WEBP',
      }),
    ).toEqual({
      prompt: 'poster',
      size: '1536x1024',
      resolution: '2k',
      background: undefined,
      outputFormat: 'webp',
      quality: undefined,
      moderation: undefined,
      outputCompression: undefined,
      count: 4,
      negativePrompt: undefined,
      seed: undefined,
      imageUrls: undefined,
      maskUrl: undefined,
    })
  })
})

describe('image_generate tool execute', () => {
  it('saves generated images into the workdir and returns image content blocks', async () => {
    const cwd = await mkdtemp(path.join(os.tmpdir(), 'image-generate-'))
    const tool = createImageGenerationTool(createDeps() as never)

    const result = await tool.execute(
      'tool-call-1',
      { prompt: 'A product poster' },
      undefined,
      undefined,
      { cwd },
    )

    expect(result.content[0].text).toContain('Generated 1 image')
    expect(result.content[0].text).toContain('Task ID: task_local_1')
    expect(result.content[1]).toMatchObject({
      type: 'image',
      mimeType: 'image/png',
    })
    const savedPath = result.details.savedPaths[0]
    expect(savedPath.startsWith(path.join(cwd, '.nineclaw-generated-images'))).toBe(true)
    expect(await readFile(savedPath, 'utf8')).toBe('fake-image')
  })

  it('returns a stable error result when the gateway env is unavailable', async () => {
    const tool = createImageGenerationTool(
      createDeps({
        processApi: { env: {}, cwd: () => process.cwd() },
      }) as never,
    )

    const result = await tool.execute(
      'tool-call-2',
      { prompt: 'A product poster' },
      undefined,
      undefined,
      { cwd: process.cwd() },
    )

    expect(result.content[0].text).toContain('Image gateway is not available')
    expect(result.details).toMatchObject({
      ok: false,
      error: 'missing image gateway env',
    })
  })
})
