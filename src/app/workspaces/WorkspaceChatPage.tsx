import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type {
  AgentRecord,
  HistoryItem,
  WorkspaceMemberView,
  WorkspaceRecord,
} from '../../types'
import { ChatView, type ChatViewProps } from '../chat/ChatWorkspace'
import { AppIcon } from '../../components/AppIcon'
import { AgentAvatar } from '../../components/AgentAvatar'
import { workspaceList } from '../../lib/piClient'
import { WorkspaceSessionsSidebar } from './WorkspaceSessionsSidebar'
import { TeamDrawer, type TeamDrawerTab } from './TeamDrawer'
import { openLlmTracePopout } from '../lib/llmTracePopout'
import {
  DelegateSegmentsContext,
  type DelegateSegmentsContextValue,
} from './chat/DelegateSegmentsBlock'

export type WorkspaceChatPageProps = Omit<ChatViewProps, 'onSubmit'> & {
  workspaceId: string
  agents: AgentRecord[]
  onBackToWorkspaces: () => void
  history: HistoryItem[]
  onSelectSession: (sessionId: string) => void
  onStartNewSession: () => void
  onDeleteSession?: (sessionId: string) => void
  /** 由 NineClawApp 提供的原生提交入口，支持 extras.overrideAgentId 透传。 */
  onChatSubmit: (
    text: string,
    extras?: { overrideAgentId?: string | null },
  ) => void | Promise<void>
  /** 委派计划卡"全部下发"—— NineClawApp 会用当前 provider runtime 调用 workspace_run_delegate_task。 */
  onDispatchDelegatePlan?: (payload: {
    workspaceId: string
    planId: string
    items: Array<{ assignee: string; task: string }>
  }) => Promise<void> | void
}

export function WorkspaceChatPage({
  workspaceId,
  agents,
  onBackToWorkspaces,
  history,
  onSelectSession,
  onStartNewSession,
  onDeleteSession,
  onChatSubmit,
  onDispatchDelegatePlan,
  ...chatProps
}: WorkspaceChatPageProps) {
  const [workspace, setWorkspace] = useState<WorkspaceRecord | null>(null)
  const [workspaceError, setWorkspaceError] = useState('')
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [drawerOpen, setDrawerOpen] = useState(false)
  const [drawerTab, setDrawerTab] = useState<TeamDrawerTab>('members')
  const [drawerMembers, setDrawerMembers] = useState<WorkspaceMemberView[]>([])
  const composerSetTextRef = useRef<((text: string) => void) | null>(null)
  const [mentionQuery, setMentionQuery] = useState<string | null>(null)
  const [composerRawText, setComposerRawText] = useState('')

  useEffect(() => {
    let cancelled = false
    setWorkspaceError('')
    workspaceList(true)
      .then((items) => {
        if (cancelled) return
        const ws = items.find((w) => w.id === workspaceId) ?? null
        setWorkspace(ws)
        if (!ws) setWorkspaceError('团队不存在或已归档')
      })
      .catch((error) => {
        if (!cancelled) setWorkspaceError(String(error))
      })
    return () => {
      cancelled = true
    }
  }, [workspaceId])

  const supervisor = useMemo(
    () => drawerMembers.find((m) => m.role === 'supervisor') ?? null,
    [drawerMembers],
  )

  const teamSessionsCount = useMemo(
    () => history.filter((item) => item.workspaceId === workspaceId).length,
    [history, workspaceId],
  )

  const memberByName = useMemo(() => {
    const map = new Map<string, WorkspaceMemberView>()
    drawerMembers.forEach((m) => {
      map.set(m.name.toLowerCase(), m)
      map.set(m.agentId.toLowerCase(), m)
    })
    return map
  }, [drawerMembers])

  const agentById = useMemo(() => {
    const map = new Map<string, AgentRecord>()
    agents.forEach((a) => map.set(a.id, a))
    return map
  }, [agents])

  /** 给 `ChatView` 的发言徽章用：agentId → 可读显示信息。 */
  const resolveSpeaker = useCallback(
    (agentId: string) => {
      const member = drawerMembers.find((m) => m.agentId === agentId)
      const agent = agentById.get(agentId)
      if (!member && !agent) return null
      const role =
        member?.role === 'supervisor'
          ? ('supervisor' as const)
          : member
            ? ('member' as const)
            : undefined
      return {
        name: member?.name ?? agent?.name ?? agentId,
        role,
        accentColor: agent?.accentColor ?? null,
        avatarUri: member?.avatarUri ?? agent?.avatarUri ?? null,
        avatarEmoji: null,
      }
    },
    [drawerMembers, agentById],
  )

  /** 只解析消息最前端的 @mention：`@foo 其余内容` → { id, body }。 */
  const resolveLeadingMention = (text: string): {
    overrideAgentId: string | null
    body: string
  } => {
    const trimmed = text.trimStart()
    if (!trimmed.startsWith('@')) return { overrideAgentId: null, body: text }
    const tokenMatch = /^@([^\s@]+)\s*/.exec(trimmed)
    if (!tokenMatch) return { overrideAgentId: null, body: text }
    const token = tokenMatch[1].toLowerCase()
    const member = memberByName.get(token)
    if (!member) return { overrideAgentId: null, body: text }
    const body = trimmed.slice(tokenMatch[0].length)
    return { overrideAgentId: member.agentId, body: body || text }
  }

  /** 旁白便签：用户补充但本次不请求 LLM；下次发送时拼到 `[USER_NOTES]` 前言里。 */
  const [pendingNotes, setPendingNotes] = useState<string[]>([])
  const [isNoteMode, setIsNoteMode] = useState(false)

  const handleWorkspaceSubmit = (text: string) => {
    if (isNoteMode) {
      const trimmed = text.trim()
      if (trimmed) setPendingNotes((prev) => [...prev, trimmed])
      composerSetTextRef.current?.('')
      setComposerRawText('')
      setMentionQuery(null)
      return
    }
    const { overrideAgentId } = resolveLeadingMention(text)
    let finalText = text
    if (pendingNotes.length > 0) {
      const header = `[USER_NOTES]\n${pendingNotes.map((n) => `- ${n}`).join('\n')}\n[/USER_NOTES]\n\n`
      finalText = `${header}${text}`
      setPendingNotes([])
    }
    void onChatSubmit(finalText, { overrideAgentId })
    setMentionQuery(null)
    setComposerRawText('')
  }

  const handleComposerInput = (value: string) => {
    setComposerRawText(value)
    const match = /(?:^|\s)@([^\s@]*)$/.exec(value)
    if (match) {
      setMentionQuery(match[1] ?? '')
    } else {
      setMentionQuery(null)
    }
  }

  const insertMention = (member: WorkspaceMemberView) => {
    const current = composerRawText
    const replaced = current.replace(/(^|\s)@[^\s@]*$/, `$1@${member.name} `)
    const next = replaced === current ? `${current}${current.endsWith(' ') || !current ? '' : ' '}@${member.name} ` : replaced
    composerSetTextRef.current?.(next)
    setComposerRawText(next)
    setMentionQuery(null)
  }

  const mentionCandidates = useMemo(() => {
    const q = (mentionQuery ?? '').toLowerCase()
    if (!q) return drawerMembers.slice(0, 8)
    return drawerMembers
      .filter(
        (m) =>
          m.name.toLowerCase().includes(q) || m.agentId.toLowerCase().includes(q),
      )
      .slice(0, 8)
  }, [drawerMembers, mentionQuery])

  const delegateCtxValue = useMemo<DelegateSegmentsContextValue>(
    () => ({
      workspaceId,
      onDispatchPlan: async ({ planId, items }) => {
        if (!onDispatchDelegatePlan) return
        await onDispatchDelegatePlan({ workspaceId, planId, items })
      },
    }),
    [workspaceId, onDispatchDelegatePlan],
  )

  const mentionPopover = mentionQuery !== null && mentionCandidates.length > 0 ? (
    <div className="workspace-mention-popover" role="listbox" aria-label="团队成员补全">
      {mentionCandidates.map((m, idx) => (
        <button
          key={m.agentId}
          type="button"
          role="option"
          aria-selected={idx === 0}
          className="workspace-mention-option"
          onClick={() => insertMention(m)}
        >
          <span className="workspace-mention-index">{idx + 1}</span>
          <AgentAvatar
            name={m.name}
            avatarUri={m.avatarUri}
            accentColor={agentById.get(m.agentId)?.accentColor ?? null}
            className="workspace-mention-avatar"
            size={14}
          />
          <span className="workspace-mention-name">{m.name}</span>
          <span className={`workspace-mention-role${m.role === 'supervisor' ? ' supervisor' : ''}`}>
            {m.role === 'supervisor' ? '主' : '成员'}
          </span>
        </button>
      ))}
    </div>
  ) : null

  const mentionChipRow = drawerMembers.length > 0 ? (
    <div className="workspace-mention-chips" role="group" aria-label="点名成员">
      <span className="workspace-mention-chips-hint">点名：</span>
      {drawerMembers.map((m) => (
        <button
          key={m.agentId}
          type="button"
          className={`workspace-mention-chip${m.role === 'supervisor' ? ' supervisor' : ''}`}
          onClick={() => insertMention(m)}
          title={m.summary || m.name}
        >
          <AgentAvatar
            name={m.name}
            avatarUri={m.avatarUri}
            accentColor={agentById.get(m.agentId)?.accentColor ?? null}
            className="workspace-mention-chip-avatar"
            size={12}
          />
          @{m.name}
        </button>
      ))}
      <button
        type="button"
        className={`workspace-note-toggle${isNoteMode ? ' is-active' : ''}`}
        onClick={() => setIsNoteMode((v) => !v)}
        title={
          isNoteMode
            ? '旁白模式：Enter 保存便签（与设置里的「发送快捷键」无关）；Shift+Enter 换行'
            : '开启后 Enter 只存便签、不请求模型；下次普通发送时把便签注入 [USER_NOTES]'
        }
      >
        {isNoteMode ? '旁白中' : '旁白'}
      </button>
    </div>
  ) : null

  const notesChipRow = pendingNotes.length > 0 ? (
    <div className="workspace-note-chips" role="group" aria-label="待注入的旁白">
      <span className="workspace-note-chips-hint">待注入：</span>
      {pendingNotes.map((note, index) => (
        <span key={index} className="workspace-note-chip" title={note}>
          <span className="workspace-note-chip-text">{note}</span>
          <button
            type="button"
            className="workspace-note-chip-remove"
            onClick={() => setPendingNotes((prev) => prev.filter((_, i) => i !== index))}
            aria-label="删除这条旁白"
          >
            ×
          </button>
        </span>
      ))}
      <button
        type="button"
        className="workspace-note-chip-clear"
        onClick={() => setPendingNotes([])}
      >
        清空
      </button>
    </div>
  ) : null

  const homeSlot = chatProps.activeHistoryItem ? null : (
    <div className="workspace-starter-cards" role="group" aria-label="团队空间启动引导">
      <button
        type="button"
        className="workspace-starter-card"
        onClick={() => {
          composerSetTextRef.current?.(
            '请作为主智能体带队：先确认项目目标与当前阶段，再把任务拆成能委派给成员的具体条目。',
          )
        }}
      >
        <div className="workspace-starter-card-head">
          <span className="workspace-starter-card-icon">
            <AppIcon name="spark" size={16} />
          </span>
          <strong>一句话告诉主智能体项目目标</strong>
        </div>
        <p className="workspace-starter-card-desc">
          把目标和约束交给主智能体，它会先整理共识，再按成员能力拆分任务。
        </p>
      </button>
      <button
        type="button"
        className="workspace-starter-card"
        onClick={() => {
          const el = document.querySelector<HTMLTextAreaElement>('.composer-textarea')
          if (el) {
            el.focus()
          }
          composerSetTextRef.current?.('@')
        }}
      >
        <div className="workspace-starter-card-head">
          <span className="workspace-starter-card-icon">
            <AppIcon name="users" size={16} />
          </span>
          <strong>@ 成员直接开聊</strong>
        </div>
        <p className="workspace-starter-card-desc">
          跳过主智能体，把问题先抛给某位成员；主智能体随后也能接力补位。
        </p>
      </button>
    </div>
  )

  const workspaceHomeTitle = workspace ? (
    <>
      <div className="workspace-home-team-name">团队 · {workspace.name}</div>
      <div className="workspace-home-team-hint">
        {teamSessionsCount > 0
          ? '从左侧选一个会话继续，或在下方开新一轮对话。'
          : '这是团队空间的启动点：在下方开场，或从引导卡起步。'}
      </div>
    </>
  ) : (
    '正在加载团队空间…'
  )

  return (
    <div className={`workspace-chat-page${drawerOpen ? ' with-drawer' : ''}`}>
      <header className="workspace-chat-topbar">
        <div className="workspace-chat-topbar-left">
          <button
            type="button"
            className="workspace-back-button"
            onClick={onBackToWorkspaces}
          >
            <AppIcon name="arrow-left" size={14} />
            <span>团队</span>
          </button>
          <div className="workspace-chat-topbar-title">
            <strong>{workspace?.name ?? '…'}</strong>
            {supervisor ? (
              <span className="workspace-chat-topbar-sup">主：{supervisor.name}</span>
            ) : null}
          </div>
        </div>
        <div className="workspace-chat-topbar-right">
          {(
            [
              { id: 'members', label: '成员', icon: 'users' },
              { id: 'resources', label: '资料', icon: 'folder' },
              { id: 'memory', label: '记忆', icon: 'book' },
              { id: 'artifacts', label: '成果', icon: 'spark' },
            ] as const
          ).map((tab) => {
            const active = drawerOpen && drawerTab === tab.id
            return (
              <button
                key={tab.id}
                type="button"
                className={`workspace-chat-topbar-button${active ? ' active' : ''}`}
                onClick={() => {
                  if (active) {
                    setDrawerOpen(false)
                  } else {
                    setDrawerTab(tab.id)
                    setDrawerOpen(true)
                  }
                }}
              >
                <AppIcon name={tab.icon} size={14} />
                <span>{tab.label}</span>
              </button>
            )
          })}
          <button
            type="button"
            className="workspace-chat-topbar-button"
            onClick={() => {
              if (workspace) {
                void openLlmTracePopout(workspace.id, chatProps.activeHistoryId || null)
              }
            }}
            title="在独立窗口打开 LLM 调用链调试"
          >
            <AppIcon name="wrench" size={14} />
            <span>调试</span>
          </button>
        </div>
      </header>

      {workspaceError ? (
        <div className="skills-feedback error agent-feedback inline workspace-chat-error">
          <span>{workspaceError}</span>
        </div>
      ) : null}

      <div className="workspace-chat-main">
        <WorkspaceSessionsSidebar
          workspaceId={workspaceId}
          history={history}
          activeHistoryId={chatProps.activeHistoryId || null}
          onSelectSession={onSelectSession}
          onStartNewSession={onStartNewSession}
          onDeleteSession={onDeleteSession}
          collapsed={sidebarCollapsed}
          onToggleCollapsed={() => setSidebarCollapsed((v) => !v)}
        />
        <div className="workspace-chat-surface">
          <DelegateSegmentsContext.Provider value={delegateCtxValue}>
            <ChatView
              {...chatProps}
              onSubmit={handleWorkspaceSubmit}
              composerSetTextRef={composerSetTextRef}
              workspaceComposerNoteMode={isNoteMode}
              workspaceComposerPlaceholder={
                isNoteMode
                  ? '旁白：Enter 保存便签，Shift+Enter 换行；关闭「旁白」后照常发送，便签会一并注入。'
                  : null
              }
              workspaceHomeSlot={homeSlot}
              workspaceHomeTitle={workspaceHomeTitle}
              workspaceComposerOverlay={
                <>
                  {notesChipRow}
                  {mentionChipRow}
                  {mentionPopover}
                </>
              }
              onComposerInput={handleComposerInput}
              resolveSpeaker={resolveSpeaker}
            />
          </DelegateSegmentsContext.Provider>
        </div>
        {workspace ? (
          <TeamDrawer
            workspace={workspace}
            agents={agents}
            open={drawerOpen}
            activeTab={drawerTab}
            onClose={() => setDrawerOpen(false)}
            onMembersChanged={setDrawerMembers}
            onWorkspaceUpdated={setWorkspace}
          />
        ) : null}
      </div>
    </div>
  )
}

export default WorkspaceChatPage
