import { invoke } from '@tauri-apps/api/core'

/** 打开 macOS 系统原生文本面板（NSTextView，便于 Fn / 豆包听写）。非 macOS 或无 Tauri 时勿调用。 */
export async function openMacNativeDictationPanel(): Promise<void> {
  return invoke('macos_open_native_dictation_panel')
}

/** 桌面 NineClaw（Tauri）且用户代理像 macOS 时显示入口。 */
export function isMacTauriComposerDesktop(): boolean {
  return (
    typeof window !== 'undefined' &&
    '__TAURI_INTERNALS__' in window &&
    /Macintosh|Mac OS X/i.test(navigator.userAgent)
  )
}
