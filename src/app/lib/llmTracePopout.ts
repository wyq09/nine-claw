/** 在独立 Webview 窗口打开 LLM 调用链调试页（与内嵌面板二选一即可看到同一数据）。 */
export async function openLlmTracePopout(
  workspaceId: string | null | undefined,
  sessionId: string | null | undefined,
): Promise<void> {
  const scopeParts = [
    workspaceId ? `ws=${encodeURIComponent(workspaceId)}` : '',
    sessionId ? `ses=${encodeURIComponent(sessionId)}` : '',
  ].filter(Boolean)
  const hashPath = `/llm-trace?${scopeParts.join('&')}`
  const labelSeed = `${workspaceId ?? 'standalone'}-${sessionId ?? 'all'}`
  const label = `llm-trace-${labelSeed.replace(/[^a-zA-Z0-9_-]/g, '_').slice(0, 48)}`
  try {
    const mod = await import('@tauri-apps/api/webviewWindow')
    const existing = await mod.WebviewWindow.getByLabel(label)
    if (existing) {
      await existing.show()
      await existing.setFocus()
      return
    }
    const win = new mod.WebviewWindow(label, {
      url: `index.html#${hashPath}`,
      title: 'LLM 调用链 · 调试',
      width: 960,
      height: 720,
      minWidth: 540,
      minHeight: 360,
      resizable: true,
      decorations: true,
    })
    win.once('tauri://error', (e: unknown) => {
      console.error('[llm-trace popout] 创建窗口失败', e)
    })
  } catch (err) {
    console.error('[llm-trace popout] 失败，回退浏览器弹窗', err)
    window.open(`${window.location.origin}/#${hashPath}`, label, 'width=960,height=720')
  }
}
