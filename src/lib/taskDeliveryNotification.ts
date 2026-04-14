import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  isPermissionGranted,
  onAction,
  registerActionTypes,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification'
import type { AgentTaskDeliveryRecord } from '../types'

let actionTypesRegistered = false

/** 与 `public/nineclaw-notification.png` 同源，供 Web Notification 使用 */
function resolveWebNotificationIconUrl(): string {
  const base = import.meta.env.BASE_URL || '/'
  return new URL('nineclaw-notification.png', window.location.origin + base).href
}

/** 打包进 bundle.resources 的图标，供 Tauri 原生通知用系统路径加载 */
async function resolveTauriBundledNotificationIconPath(): Promise<string | undefined> {
  try {
    const { resourceDir, join } = await import('@tauri-apps/api/path')
    const root = await resourceDir()
    return await join(root, 'icons', '128x128.png')
  } catch {
    return undefined
  }
}

async function ensureMobileActionTypes(): Promise<void> {
  if (actionTypesRegistered) {
    return
  }
  try {
    await registerActionTypes([
      {
        id: 'nineclaw-agent-task',
        actions: [{ id: 'open', title: '查看对话', foreground: true }],
      },
    ])
    actionTypesRegistered = true
  } catch {
    // Desktop / unsupported: ignore
  }
}

async function focusMainWindow(): Promise<void> {
  try {
    const win = getCurrentWindow()
    await win.unminimize()
    await win.show()
    await win.setFocus()
  } catch {
    // ignore
  }
}

/**
 * 系统级任务完成提醒：优先使用 Web Notification（桌面端 WebView 通常支持 onclick 跳转会话）；
 * 否则回退到 Tauri 原生 toast（部分平台点击无法回调，仅作提醒）。
 */
export async function showTaskDeliveryDesktopNotification(
  payload: AgentTaskDeliveryRecord,
  openSession: () => void,
): Promise<void> {
  const title = `定时任务：${payload.title}`.trim() || '定时任务'
  const raw = payload.content.trim() || '任务已执行，点击查看详情。'
  const body = raw.length > 180 ? `${raw.slice(0, 180)}…` : raw

  const runOpen = () => {
    void focusMainWindow()
    openSession()
  }

  if (typeof window !== 'undefined' && 'Notification' in window) {
    try {
      let perm = Notification.permission
      if (perm === 'default') {
        perm = await Notification.requestPermission()
      }
      if (perm === 'granted') {
        const n = new Notification(title, {
          body,
          tag: `nineclaw-task-${payload.id}`,
          icon: resolveWebNotificationIconUrl(),
        })
        n.onclick = (ev) => {
          ev.preventDefault()
          n.close()
          runOpen()
        }
        return
      }
    } catch {
      // fall through to Tauri
    }
  }

  await ensureMobileActionTypes()

  try {
    let granted = await isPermissionGranted()
    if (!granted) {
      const r = await requestPermission()
      granted = r === 'granted'
    }
    if (granted) {
      const iconPath = await resolveTauriBundledNotificationIconPath()
      sendNotification({
        title,
        body,
        ...(iconPath ? { icon: iconPath } : {}),
        actionTypeId: 'nineclaw-agent-task',
        extra: { sessionId: payload.sessionId, deliveryId: payload.id },
      })
    }
  } catch {
    // ignore
  }
}

/** 注册通知动作回调（移动端点「查看对话」；桌面端部分环境也可能触发）。 */
export async function listenTaskDeliveryNotificationActions(
  openSessionById: (sessionId: string) => void,
): Promise<() => void> {
  await ensureMobileActionTypes()
  const listener = await onAction((opt) => {
    const sid = opt.extra?.sessionId
    if (typeof sid === 'string' && sid.trim()) {
      void focusMainWindow()
      openSessionById(sid.trim())
    }
  })
  return () => {
    void listener.unregister()
  }
}
