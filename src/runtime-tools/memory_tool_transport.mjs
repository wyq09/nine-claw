export async function parseProxyJsonResponse(response) {
  if (typeof response?.text === 'function') {
    const text = await response.text()
    try {
      return {
        ok: response.ok,
        status: response.status,
        data: text ? JSON.parse(text) : {},
        rawText: text,
      }
    } catch {
      return {
        ok: response.ok,
        status: response.status,
        data: {
          ok: response.ok,
          status: response.status,
          error: text || `HTTP ${response.status}`,
        },
        rawText: text,
      }
    }
  }
  if (typeof response?.json === 'function') {
    const data = await response.json()
    return {
      ok: response.ok,
      status: response.status,
      data,
      rawText: typeof data === 'string' ? data : JSON.stringify(data),
    }
  }
  const text = ''
  try {
    return {
      ok: response?.ok ?? false,
      status: response?.status ?? 0,
      data: text ? JSON.parse(text) : {},
      rawText: text,
    }
  } catch {
    return {
      ok: response?.ok ?? false,
      status: response?.status ?? 0,
      data: {
        ok: response?.ok ?? false,
        status: response?.status ?? 0,
        error: text || `HTTP ${response?.status ?? 0}`,
      },
      rawText: text,
    }
  }
}
