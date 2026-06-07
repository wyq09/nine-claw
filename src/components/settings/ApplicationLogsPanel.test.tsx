import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApplicationLogsPanel } from './ApplicationLogsPanel'
import { appLogList, appLogRead } from '../../lib/appLogClient'

vi.mock('../../lib/appLogClient', () => ({
  appLogExportAll: vi.fn().mockResolvedValue(2),
  appLogList: vi.fn(),
  appLogOpenDir: vi.fn().mockResolvedValue(undefined),
  appLogRead: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

const logContent = [
  '[2026-06-06T10:00:00Z] [INFO] [app] boot complete',
  '[2026-06-06T10:01:00Z] [WARN] [runtime] slow startup',
  '[2026-06-06T10:02:00Z] [ERROR] [provider] {"message":"failed","code":500}',
].join('\n')

describe('ApplicationLogsPanel', () => {
  beforeEach(() => {
    vi.mocked(appLogList).mockResolvedValue({
      dir: '/tmp/nineclaw/logs',
      files: [
        { name: 'nineclaw-2026-06-06.log', sizeBytes: 4096 },
        { name: 'nineclaw-2026-06-05.log', sizeBytes: 2048 },
      ],
      totalBytes: 6144,
    })
    vi.mocked(appLogRead).mockImplementation(async (fileName: string) => ({
      content:
        fileName.includes('06-05')
          ? '[2026-06-05T10:00:00Z] [INFO] [app] older log'
          : logContent,
      truncated: false,
      fileSizeBytes: 4096,
    }))
  })

  it('renders a sidebar file list with a separate log viewport', async () => {
    const { container } = render(<ApplicationLogsPanel />)

    expect(await screen.findByRole('heading', { name: '应用日志' })).toBeInTheDocument()
    expect(container.querySelector('.app-log-workspace')).toBeTruthy()
    expect(screen.getByRole('complementary', { name: '日志筛选' })).toBeInTheDocument()
    expect(screen.getByRole('log')).toBeInTheDocument()
    expect(await screen.findByText('boot complete')).toBeInTheDocument()
  })

  it('switches files and filters visible log lines', async () => {
    render(<ApplicationLogsPanel />)

    const fileList = await screen.findByRole('listbox', { name: '日志文件' })
    expect(await screen.findByText('boot complete')).toBeInTheDocument()
    fireEvent.click(within(fileList).getByRole('option', { name: /2026-06-05/ }))
    expect(await screen.findByText('older log')).toBeInTheDocument()

    fireEvent.click(within(fileList).getByRole('option', { name: /2026-06-06/ }))
    expect(await screen.findByText('slow startup')).toBeInTheDocument()

    fireEvent.change(screen.getByRole('combobox', { name: '按级别筛选' }), {
      target: { value: 'error' },
    })
    expect(within(screen.getByRole('log')).getByText('ERROR')).toBeInTheDocument()
    expect(screen.queryByText('slow startup')).not.toBeInTheDocument()

    fireEvent.change(screen.getByRole('searchbox'), {
      target: { value: 'nomatch' },
    })
    await waitFor(() => {
      expect(screen.getByText('没有匹配的日志行。尝试调整筛选条件或刷新。')).toBeInTheDocument()
    })
  })

  it('shows a truncated notice when only the file tail is loaded', async () => {
    vi.mocked(appLogRead).mockResolvedValue({
      content: '[2026-06-06T10:00:00Z] [INFO] [app] tail only',
      truncated: true,
      fileSizeBytes: 2_000_000,
    })

    render(<ApplicationLogsPanel />)

    expect(await screen.findByText(/默认只加载末尾约 512 KB/)).toBeInTheDocument()
  })
})
