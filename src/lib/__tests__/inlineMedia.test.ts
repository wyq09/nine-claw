import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: vi.fn((value: string) => `asset:${value}`),
}))

import { extractInlineMediaAttachments } from '../inlineMedia'

describe('extractInlineMediaAttachments', () => {
  it('salvages an encoded nested image directive inside a malformed file directive', () => {
    const content =
      '::nc-media{type="file" path="%E6%88%91%E5%85%88%E5%8F%91%E7%AC%AC%E4%B8%80%E5%BC%A0%E7%BB%99%E4%BD%A0%E7%A1%AE%E8%AE%A4%E6%98%AF%E4%B8%8D%E6%98%AF%E4%B8%8A%E6%9D%8E%E6%B0%B4%E5%BA%93%E7%9A%84%E5%9C%BA%E6%99%AF%E3%80%82::nc-media%7Btype%3D%22image%22%20path%3D%22/Users/yiqunwu/.nineclaw/workspace/chat-sessions/1780410723027-c2hbncel/CV8I2311_%E6%B9%96%E8%BE%B9%E8%8D%89%E5%9C%B0.JPG%22%7D" name="CV8I2311_%E6%B9%96%E8%BE%B9%E8%8D%89%E5%9C%B0.JPG%22%7D" label="%E6%96%87%E4%BB%B6"}'

    const result = extractInlineMediaAttachments(content)

    expect(result.attachments).toHaveLength(1)
    expect(result.attachments[0]).toEqual(
      expect.objectContaining({
        kind: 'image',
        path: '/Users/yiqunwu/.nineclaw/workspace/chat-sessions/1780410723027-c2hbncel/CV8I2311_湖边草地.JPG',
        fileName: 'CV8I2311_湖边草地.JPG',
        label: '图片',
      }),
    )
    expect(result.contentWithoutAttachments).toBe('')
  })
})
