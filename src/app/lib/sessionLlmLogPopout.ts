export async function openSessionLlmLogPopout(
  workspaceId: string | null | undefined,
  sessionId: string | null | undefined,
): Promise<void> {
  const scopeParts = [
    workspaceId ? `ws=${encodeURIComponent(workspaceId)}` : '',
    sessionId ? `ses=${encodeURIComponent(sessionId)}` : '',
  ].filter(Boolean)
  const hashPath = `/session-llm-log?${scopeParts.join('&')}`
  const labelSeed = `${workspaceId ?? 'standalone'}-${sessionId ?? 'all'}`
  const label = `session-llm-log-${labelSeed.replace(/[^a-zA-Z0-9_-]/g, '_').slice(0, 44)}`
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
      title: 'Session 文本日志',
      width: 1080,
      height: 760,
      minWidth: 680,
      minHeight: 420,
      resizable: true,
      decorations: true,
    })
    win.once('tauri://error', (e: unknown) => {
      console.error('[session llm log popout] 创建窗口失败', e)
    })
  } catch (err) {
    console.error('[session llm log popout] 失败，回退浏览器弹窗', err)
    window.open(`${window.location.origin}/#${hashPath}`, label, 'width=1080,height=760')
  }
}
