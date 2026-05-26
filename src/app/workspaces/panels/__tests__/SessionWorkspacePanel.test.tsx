import { open } from '@tauri-apps/plugin-dialog'
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { SessionWorkspacePanel } from '../SessionWorkspacePanel'
import { computeWorkspaceMenuAnchor } from '../SessionWorkspaceOverlays'
import {
  sessionWorkspaceAbsolutePath,
  sessionWorkspaceGet,
  sessionWorkspaceImportFiles,
  sessionWorkspaceListEntries,
  sessionWorkspaceReadFile,
  sessionWorkspaceRevealPath,
} from '../../../../lib/sessionWorkspaceClient'

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

vi.mock('../../../../components/MarkdownRenderer', () => ({
  default: ({ content }: { content: string }) => <article>{content}</article>,
}))

vi.mock('../../../../lib/piClient', () => ({
  loadLocalMediaPreview: vi.fn().mockResolvedValue('asset://image'),
}))

vi.mock('../../../../lib/sessionWorkspaceClient', () => ({
  sessionWorkspaceAbsolutePath: vi.fn(),
  sessionWorkspaceCreateDir: vi.fn(),
  sessionWorkspaceCreateFile: vi.fn(),
  sessionWorkspaceDelete: vi.fn(),
  sessionWorkspaceGet: vi.fn(),
  sessionWorkspaceImportFiles: vi.fn(),
  sessionWorkspaceListEntries: vi.fn(),
  sessionWorkspaceOpenPath: vi.fn(),
  sessionWorkspaceReadFile: vi.fn(),
  sessionWorkspaceRename: vi.fn(),
  sessionWorkspaceResetToTopic: vi.fn(),
  sessionWorkspaceRevealPath: vi.fn(),
  sessionWorkspaceSwitch: vi.fn(),
}))

const mockGet = vi.mocked(sessionWorkspaceGet)
const mockList = vi.mocked(sessionWorkspaceListEntries)
const mockRead = vi.mocked(sessionWorkspaceReadFile)
const mockImportFiles = vi.mocked(sessionWorkspaceImportFiles)
const mockOpenDialog = vi.mocked(open)
const mockAbsolutePath = vi.mocked(sessionWorkspaceAbsolutePath)
const mockRevealPath = vi.mocked(sessionWorkspaceRevealPath)

const state = {
  sessionId: 'session-1',
  topicWorkspaceDir: '/tmp/topic',
  currentWorkspaceDir: '/tmp/topic',
  currentIsTopic: true,
  recents: [],
}

const rootEntries = [
  {
    name: 'src',
    relPath: 'src',
    isDir: true,
    size: null,
    modifiedMs: null,
  },
  {
    name: 'README.md',
    relPath: 'README.md',
    isDir: false,
    size: 12,
    modifiedMs: null,
  },
]

const srcEntries = [
  {
    name: 'main.ts',
    relPath: 'src/main.ts',
    isDir: false,
    size: 21,
    modifiedMs: null,
  },
]

function setupClientMocks() {
  mockGet.mockResolvedValue(state)
  mockList.mockImplementation(async (_sessionId, subPath) => {
    if (subPath === 'src') {
      return srcEntries
    }
    return rootEntries
  })
  mockRead.mockImplementation(async (_sessionId, relPath) => ({
    info: {
      name: relPath.split('/').pop() || relPath,
      relPath,
      absolutePath: `/tmp/topic/${relPath}`,
      isDir: false,
      size: 10,
      modifiedMs: null,
    },
    content: relPath.endsWith('.md') ? '# Hello' : 'const answer = 42\n',
    previewKind: relPath.endsWith('.md') ? 'markdown' : 'code',
  }))
  mockAbsolutePath.mockResolvedValue('/tmp/topic/README.md')
  mockRevealPath.mockResolvedValue(undefined)
  mockImportFiles.mockResolvedValue([
    {
      name: 'notes.md',
      relPath: 'notes.md',
      absolutePath: '/tmp/topic/notes.md',
      isDir: false,
      size: 12,
      modifiedMs: null,
    },
  ])
  mockOpenDialog.mockResolvedValue(['/tmp/import/notes.md'])
}

describe('computeWorkspaceMenuAnchor', () => {
  it('opens the workspace menu downward by default', () => {
    const anchor = computeWorkspaceMenuAnchor({
      top: 48,
      bottom: 72,
      left: 120,
      width: 280,
      right: 400,
      height: 24,
      x: 120,
      y: 48,
      toJSON: () => ({}),
    })

    expect(anchor.placement).toBe('bottom')
    expect(anchor.top).toBe(76)
    expect(anchor.maxHeight).toBeGreaterThan(120)
  })
})

describe('SessionWorkspacePanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    const storage = new Map<string, string>()
    vi.stubGlobal('localStorage', {
      getItem: vi.fn((key: string) => storage.get(key) ?? null),
      setItem: vi.fn((key: string, value: string) => {
        storage.set(key, value)
      }),
      removeItem: vi.fn((key: string) => {
        storage.delete(key)
      }),
      clear: vi.fn(() => {
        storage.clear()
      }),
    })
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: vi.fn().mockResolvedValue(undefined),
      },
    })
    setupClientMocks()
  })

  it('expands folders and opens code previews from the file tree', async () => {
    const { container } = render(<SessionWorkspacePanel sessionId="session-1" modelLabel="gpt-test" />)
    const tree = container.querySelector('.session-workspace-tree-pane') as HTMLElement

    await screen.findByRole('button', { name: /README.md/ })
    fireEvent.click(within(tree).getByRole('button', { name: /src/ }))

    await within(tree).findByRole('button', { name: /main.ts/ })
    fireEvent.click(within(tree).getByRole('button', { name: /main.ts/ }))

    await screen.findByText('const answer = 42')
    expect(mockList).toHaveBeenCalledWith('session-1', 'src')
    expect(mockRead).toHaveBeenCalledWith('session-1', 'src/main.ts')
  })

  it('closes preview tabs without closing the remaining tab', async () => {
    const { container } = render(<SessionWorkspacePanel sessionId="session-1" modelLabel="gpt-test" />)
    const tree = container.querySelector('.session-workspace-tree-pane') as HTMLElement

    await screen.findByRole('button', { name: /README.md/ })
    fireEvent.click(within(tree).getByRole('button', { name: /README.md/ }))
    await screen.findByText('# Hello')

    fireEvent.click(within(tree).getByRole('button', { name: /src/ }))
    await within(tree).findByRole('button', { name: /main.ts/ })
    fireEvent.click(within(tree).getByRole('button', { name: /main.ts/ }))
    await screen.findByText('const answer = 42')

    fireEvent.click(screen.getByRole('button', { name: '关闭 main.ts' }))

    expect(screen.queryByRole('button', { name: '关闭 main.ts' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '关闭 README.md' })).toBeInTheDocument()
  })

  it('collapses and expands the working directory column independently', async () => {
    render(<SessionWorkspacePanel sessionId="session-1" modelLabel="gpt-test" />)

    await screen.findByRole('button', { name: /README.md/ })
    expect(screen.getByText('工作目录')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '折叠工作目录' }))
    expect(screen.getByText('工作目录')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /README.md/ })).not.toBeInTheDocument()
    expect(document.querySelector('.session-workspace-body.tree-collapsed')).toBeTruthy()
    expect(document.querySelector('.session-workspace-tree-pane')).not.toBeInTheDocument()
    expect(document.querySelector('.session-workspace-tree-scroll')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '展开工作目录' }))
    expect(screen.getByText('工作目录')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /README.md/ })).toBeInTheDocument()
  })

  it('imports selected files into the workspace from the add button', async () => {
    render(<SessionWorkspacePanel sessionId="session-1" modelLabel="gpt-test" />)

    await screen.findByText('工作目录')
    fireEvent.click(screen.getByRole('button', { name: '添加文件' }))

    await waitFor(() => {
      expect(mockOpenDialog).toHaveBeenCalledWith({
        multiple: true,
        directory: false,
        title: '添加文件到工作目录',
      })
      expect(mockImportFiles).toHaveBeenCalledWith({
        sessionId: 'session-1',
        sourcePaths: ['/tmp/import/notes.md'],
        parentRel: '',
      })
    })
    await screen.findByText('# Hello')
  })

  it('renders the context menu above the preview pane via portal', async () => {
    render(<SessionWorkspacePanel sessionId="session-1" modelLabel="gpt-test" />)

    const readmeRow = await screen.findByRole('button', { name: /README.md/ })
    fireEvent.contextMenu(readmeRow, { clientX: 10, clientY: 20 })

    const menu = screen.getByText('添加到会话窗口').closest('.session-workspace-context-menu')
    expect(menu).not.toBeNull()
    expect(menu?.parentElement).toBe(document.body)
  })

  it('opens workspace directory menu from the path selector next to model label', async () => {
    render(<SessionWorkspacePanel sessionId="session-1" modelLabel="gpt-test" />)

    await screen.findByText('gpt-test')
    fireEvent.click(screen.getByRole('button', { name: '选择工作区目录' }))

    const menu = screen.getByText('切换到其他文件夹…').closest('.session-workspace-menu')
    expect(menu).not.toBeNull()
    expect(menu?.parentElement).toBe(document.body)
    expect(menu?.getAttribute('data-placement')).toBe('bottom')
    expect(document.querySelector('.session-workspace-overlay-backdrop')).toBeTruthy()
    expect(screen.getByText('在 Finder 中打开')).toBeInTheDocument()
  })

  it('truncates long preview tab labels while keeping full name in title', async () => {
    mockList.mockResolvedValue([
      {
        name: 'very-long-file-name-that-should-be-truncated-in-tab.md',
        relPath: 'very-long-file-name-that-should-be-truncated-in-tab.md',
        isDir: false,
        size: 12,
        modifiedMs: null,
      },
    ])
    mockRead.mockResolvedValue({
      info: {
        name: 'very-long-file-name-that-should-be-truncated-in-tab.md',
        relPath: 'very-long-file-name-that-should-be-truncated-in-tab.md',
        absolutePath: '/tmp/topic/very-long-file-name-that-should-be-truncated-in-tab.md',
        isDir: false,
        size: 10,
        modifiedMs: null,
      },
      content: '# Long name',
      previewKind: 'markdown',
    })

    const { container } = render(<SessionWorkspacePanel sessionId="session-1" modelLabel="gpt-test" />)
    const tree = container.querySelector('.session-workspace-tree-pane') as HTMLElement

    fireEvent.click(await within(tree).findByRole('button', { name: /very-long-file-name/ }))

    await waitFor(() => {
      const tab = container.querySelector('.session-workspace-tabs button.active') as HTMLButtonElement
      expect(tab).toBeTruthy()
      expect(tab).toHaveAttribute('title', 'very-long-file-name-that-should-be-truncated-in-tab.md')
      expect(tab.querySelector('.session-workspace-tab-label')).toBeTruthy()
    })
  })

  it('runs context menu actions for composer insertion and path copying', async () => {
    const addToComposer = vi.fn()
    render(
      <SessionWorkspacePanel
        sessionId="session-1"
        modelLabel="gpt-test"
        onAddToComposer={addToComposer}
      />,
    )

    const readmeRow = await screen.findByRole('button', { name: /README.md/ })
    fireEvent.contextMenu(readmeRow, { clientX: 10, clientY: 20 })

    const menu = screen.getByText('添加到会话窗口').closest('.session-workspace-context-menu')
    expect(menu).not.toBeNull()
    fireEvent.click(within(menu as HTMLElement).getByText('添加到会话窗口'))
    expect(addToComposer).toHaveBeenCalledWith('请参考会话工作区文件：README.md')

    fireEvent.contextMenu(readmeRow, { clientX: 10, clientY: 20 })
    fireEvent.click(screen.getByText('复制绝对路径'))
    await waitFor(() => {
      expect(mockAbsolutePath).toHaveBeenCalledWith('session-1', 'README.md', true)
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith('/tmp/topic/README.md')
    })

    fireEvent.contextMenu(readmeRow, { clientX: 10, clientY: 20 })
    fireEvent.click(screen.getByText('在 Finder 中显示'))
    await waitFor(() => expect(mockRevealPath).toHaveBeenCalledWith('session-1', 'README.md'))
  })
})
