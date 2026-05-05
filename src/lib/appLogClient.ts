import { invoke } from '@tauri-apps/api/core'

export type AppLogFileMeta = {
  name: string
  sizeBytes: number
}

export type AppLogsOverview = {
  dir: string
  files: AppLogFileMeta[]
  totalBytes: number
}

export async function appLogList(): Promise<AppLogsOverview> {
  return invoke<AppLogsOverview>('app_log_list')
}

export async function appLogRead(fileName: string): Promise<string> {
  return invoke<string>('app_log_read', { fileName })
}

export async function appLogOpenDir(): Promise<void> {
  return invoke('app_log_open_dir')
}

export async function appLogExportAll(destDir: string): Promise<number> {
  return invoke<number>('app_log_export_all', { destDir })
}
