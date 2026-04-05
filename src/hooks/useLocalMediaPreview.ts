import { useEffect, useState } from 'react'
import { loadLocalMediaPreview } from '../lib/piClient'

const previewCache = new Map<string, Promise<string> | string>()

function isLocalPreviewTarget(value: string): boolean {
  const trimmed = value.trim()
  return Boolean(trimmed) && (/^file:\/\//i.test(trimmed) || trimmed.startsWith('/') || /^[A-Za-z]:[\\/]/.test(trimmed))
}

export function useLocalMediaPreview({
  path,
  mimeType,
  fallbackSrc,
  enabled = true,
}: {
  path: string
  mimeType?: string | null
  fallbackSrc: string
  enabled?: boolean
}) {
  const [previewSrc, setPreviewSrc] = useState(fallbackSrc)

  useEffect(() => {
    setPreviewSrc(fallbackSrc)

    const trimmedPath = path.trim()
    if (!enabled || !isLocalPreviewTarget(trimmedPath)) {
      return
    }

    const cacheKey = `${trimmedPath}::${mimeType ?? ''}`
    const cached = previewCache.get(cacheKey)
    const request =
      typeof cached === 'string'
        ? Promise.resolve(cached)
        : cached ??
          loadLocalMediaPreview(trimmedPath, mimeType)
            .then((result) => {
              previewCache.set(cacheKey, result)
              return result
            })
            .catch((error) => {
              previewCache.delete(cacheKey)
              throw error
            })

    if (!cached) {
      previewCache.set(cacheKey, request)
    }

    let cancelled = false
    void request
      .then((result) => {
        if (!cancelled) {
          setPreviewSrc(result)
        }
      })
      .catch(() => {
        if (!cancelled) {
          setPreviewSrc(fallbackSrc)
        }
      })

    return () => {
      cancelled = true
    }
  }, [enabled, fallbackSrc, mimeType, path])

  return previewSrc
}
