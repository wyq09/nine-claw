import { beforeEach, describe, expect, it, vi } from 'vitest'

const pluginMocks = vi.hoisted(() => ({
  isPermissionGranted: vi.fn(),
  onAction: vi.fn(),
  registerActionTypes: vi.fn(),
  requestPermission: vi.fn(),
  sendNotification: vi.fn(),
}))

const coreMocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-notification', () => pluginMocks)
vi.mock('@tauri-apps/api/core', () => coreMocks)

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(() => ({
    unminimize: vi.fn(),
    show: vi.fn(),
    setFocus: vi.fn(),
  })),
}))

describe('taskDeliveryNotification permission state', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.clearAllMocks()
    vi.stubGlobal('localStorage', {
      getItem: vi.fn().mockReturnValue(null),
      setItem: vi.fn(),
      removeItem: vi.fn(),
    })
    vi.stubGlobal('__TAURI_INTERNALS__', {})
  })

  it('treats missing prior prompt as not_determined when native permission is not granted', async () => {
    pluginMocks.isPermissionGranted.mockResolvedValue(false)
    const { getNotificationPermissionState } = await import('../taskDeliveryNotification')

    await expect(getNotificationPermissionState()).resolves.toBe('not_determined')
  })

  it('treats missing native grant as denied after a prompt has already been attempted', async () => {
    vi.stubGlobal('localStorage', {
      getItem: vi.fn().mockReturnValue('1'),
      setItem: vi.fn(),
      removeItem: vi.fn(),
    })
    pluginMocks.isPermissionGranted.mockResolvedValue(false)
    const { getNotificationPermissionState } = await import('../taskDeliveryNotification')

    await expect(getNotificationPermissionState()).resolves.toBe('denied')
  })

  it('marks permission as requested when asking native notification permission', async () => {
    const setItem = vi.fn()
    vi.stubGlobal('localStorage', {
      getItem: vi.fn().mockReturnValue(null),
      setItem,
      removeItem: vi.fn(),
    })
    pluginMocks.isPermissionGranted.mockResolvedValue(false)
    pluginMocks.requestPermission.mockResolvedValue('granted')
    const { requestNotificationPermission } = await import('../taskDeliveryNotification')

    await expect(requestNotificationPermission()).resolves.toBe('granted')
    expect(pluginMocks.requestPermission).toHaveBeenCalledTimes(1)
    expect(setItem).toHaveBeenCalledWith('nineclaw.notification.permission.requested', '1')
  })

  it('sends Tauri app notifications before using the native fallback', async () => {
    pluginMocks.registerActionTypes.mockResolvedValue(undefined)
    pluginMocks.isPermissionGranted.mockResolvedValue(true)
    const { showAgentReplyNotification } = await import('../taskDeliveryNotification')

    await showAgentReplyNotification('session-1', '任务完成')

    expect(pluginMocks.sendNotification).toHaveBeenCalledWith(
      expect.objectContaining({
        title: '任务完成',
        body: 'Agent 回复完成，点击查看详情。',
        extra: { sessionId: 'session-1' },
      }),
    )
    expect(coreMocks.invoke).not.toHaveBeenCalledWith('send_native_notification', expect.anything())
  })

  it('falls back to the Rust native notification command when app notification fails', async () => {
    pluginMocks.registerActionTypes.mockResolvedValue(undefined)
    pluginMocks.isPermissionGranted.mockResolvedValue(true)
    pluginMocks.sendNotification.mockImplementation(() => {
      throw new Error('plugin unavailable')
    })
    coreMocks.invoke.mockResolvedValue(undefined)
    const { sendTestNotification } = await import('../taskDeliveryNotification')

    await sendTestNotification()

    expect(coreMocks.invoke).toHaveBeenCalledWith('send_native_notification', {
      title: 'NineClaw 测试通知',
      body: '如果你看到这条通知，说明系统通知链路已可用。',
    })
  })

  it('returns a no-op unregister when action listeners are unsupported', async () => {
    pluginMocks.registerActionTypes.mockRejectedValue(new Error('unsupported'))
    pluginMocks.onAction.mockRejectedValue(new Error('unsupported'))
    const { listenTaskDeliveryNotificationActions } = await import('../taskDeliveryNotification')

    const unlisten = await listenTaskDeliveryNotificationActions(() => {})

    expect(typeof unlisten).toBe('function')
    expect(() => unlisten()).not.toThrow()
  })
})
