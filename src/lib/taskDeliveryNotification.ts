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
const NOTIFICATION_PERMISSION_REQUESTED_KEY = 'nineclaw.notification.permission.requested'

/** 是否运行在 Tauri 原生壳内 */
const isTauri = '__TAURI_INTERNALS__' in window

// ---------- 权限管理 ----------

export type NotificationPermissionState = 'granted' | 'denied' | 'not_determined'

function hasRequestedNotificationPermission(): boolean {
  try {
    return window.localStorage.getItem(NOTIFICATION_PERMISSION_REQUESTED_KEY) === '1'
  } catch {
    return false
  }
}

function markNotificationPermissionRequested(): void {
  try {
    window.localStorage.setItem(NOTIFICATION_PERMISSION_REQUESTED_KEY, '1')
  } catch {
    // ignore
  }
}

/** 查询当前系统通知权限状态 */
export async function getNotificationPermissionState(): Promise<NotificationPermissionState> {
  if (!isTauri) {
    if ('Notification' in window) {
      const p = Notification.permission
      if (p === 'granted') return 'granted'
      if (p === 'denied') return 'denied'
    }
    return 'not_determined'
  }
  try {
    const granted = await isPermissionGranted()
    if (granted) {
      return 'granted'
    }
    return hasRequestedNotificationPermission() ? 'denied' : 'not_determined'
  } catch {
    return hasRequestedNotificationPermission() ? 'denied' : 'not_determined'
  }
}

/**
 * 请求系统通知权限（首次调用会触发 macOS 系统弹窗）。
 * 返回授权后的状态。
 */
export async function requestNotificationPermission(): Promise<NotificationPermissionState> {
  if (!isTauri) {
    if ('Notification' in window) {
      markNotificationPermissionRequested()
      const p = await Notification.requestPermission()
      return p === 'granted' ? 'granted' : 'denied'
    }
    return 'not_determined'
  }
  try {
    const granted = await isPermissionGranted()
    if (granted) {
      markNotificationPermissionRequested()
      return 'granted'
    }
    const r = await requestPermission()
    markNotificationPermissionRequested()
    return r === 'granted' ? 'granted' : 'denied'
  } catch {
    return 'not_determined'
  }
}

/** 打开 macOS 系统设置 → 通知面板（引导用户手动开启） */
export async function openSystemNotificationSettings(): Promise<void> {
  if (!isTauri) return
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('open_system_notification_settings')
  } catch (err) {
    console.warn('[notification] openSystemNotificationSettings failed:', err)
  }
}

/** 与 `public/nineclaw-notification.png` 同源，供 Web Notification 使用 */
function resolveWebNotificationIconUrl(): string {
  const base = import.meta.env.BASE_URL || '/'
  return new URL('nineclaw-notification.png', window.location.origin + base).href
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

async function sendTauriAppNotification(options: {
  title: string
  body: string
  extra?: Record<string, string>
}): Promise<boolean> {
  try {
    await ensureMobileActionTypes()
    let granted = await isPermissionGranted()
    if (!granted) {
      const permission = await requestPermission()
      markNotificationPermissionRequested()
      granted = permission === 'granted'
    }
    if (!granted) {
      return false
    }
    sendNotification({
      title: options.title,
      body: options.body,
      actionTypeId: 'nineclaw-agent-task',
      extra: options.extra,
    })
    return true
  } catch (err) {
    console.warn('[notification] Tauri app notification failed:', err)
    return false
  }
}

async function sendNativeFallbackNotification(title: string, body: string): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('send_native_notification', { title, body })
  } catch (err) {
    console.warn('[notification] native fallback failed:', err)
  }
}

export async function sendTestNotification(): Promise<void> {
  const title = 'NineClaw 测试通知'
  const body = '如果你看到这条通知，说明系统通知链路已可用。'
  if (isTauri) {
    const sent = await sendTauriAppNotification({ title, body })
    if (!sent) {
      await sendNativeFallbackNotification(title, body)
    }
    return
  }

  if (typeof window !== 'undefined' && 'Notification' in window) {
    let perm = Notification.permission
    if (perm === 'default') {
      perm = await Notification.requestPermission()
    }
    if (perm === 'granted') {
      new Notification(title, { body, icon: resolveWebNotificationIconUrl() })
    }
  }
}

/**
 * 系统级任务完成提醒。
 * macOS 上用 osascript（display notification），兼容 macOS 15+。
 */
export async function showTaskDeliveryDesktopNotification(
  payload: AgentTaskDeliveryRecord,
  openSession: () => void,
): Promise<void> {
  const title = `定时任务：${payload.title}`.trim() || '定时任务'
  const raw = payload.content.trim() || '任务已执行，点击查看详情。'
  const body = raw.length > 180 ? `${raw.slice(0, 180)}…` : raw

  if (isTauri) {
    const sent = await sendTauriAppNotification({
      title,
      body,
      extra: { sessionId: payload.sessionId, deliveryId: payload.id },
    })
    if (!sent) {
      await sendNativeFallbackNotification(title, body)
    }
    return
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
        n.onclick = () => {
          n.close()
          void focusMainWindow()
          openSession()
        }
        return
      }
    } catch {
      // fall through
    }
  }
}

/**
 * Agent 回复完成时的轻量通知。
 *
 * macOS 策略：
 * 1. 优先用 osascript（display notification）— 不依赖 notify-rust，
 *    兼容 macOS 15+（NSUserNotificationCenter 已移除）
 * 2. osascript 通知不支持点击回调，点击后无法自动跳转
 *
 * 非 Tauri（纯浏览器）走 Web Notification。
 */
export async function showAgentReplyNotification(
  sessionId: string,
  sessionTitle: string,
): Promise<void> {
  const title = sessionTitle.trim() || 'NineClaw'
  const body = 'Agent 回复完成，点击查看详情。'

  if (isTauri) {
    const sent = await sendTauriAppNotification({
      title,
      body,
      extra: { sessionId },
    })
    if (!sent) {
      await sendNativeFallbackNotification(title, body)
    }
    return
  }

  // 非 Tauri（纯浏览器）走 Web Notification
  if (typeof window !== 'undefined' && 'Notification' in window) {
    try {
      let perm = Notification.permission
      if (perm === 'default') {
        perm = await Notification.requestPermission()
      }
      if (perm === 'granted') {
        const n = new Notification(title, {
          body,
          tag: `nineclaw-reply-${sessionId}`,
          icon: resolveWebNotificationIconUrl(),
        })
        n.onclick = () => {
          n.close()
          void focusMainWindow()
          window.dispatchEvent(
            new CustomEvent('nineclaw-navigate-session', { detail: { sessionId } }),
          )
        }
        return
      }
    } catch {
      // ignore
    }
  }
}

/** 注册通知动作回调（移动端点「查看对话」；桌面端部分环境也可能触发）。 */
export async function listenTaskDeliveryNotificationActions(
  openSessionById: (sessionId: string) => void,
): Promise<() => void> {
  try {
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
  } catch {
    return () => {}
  }
}
