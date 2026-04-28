import { useCallback, useEffect, useMemo, useState, type ChangeEvent, type RefObject } from 'react'
import type {
  AgentRecord,
  WorkspaceMemberView,
  WorkspaceMemoryRecord,
  WorkspaceRecord,
  WorkspaceResourceRecord,
} from '../../types'
import {
  workspaceAddMember,
  workspaceDeleteMemory,
  workspaceDeleteResource,
  workspaceList,
  workspaceListMembers,
  workspaceListMemories,
  workspaceListResources,
  workspaceRemoveMember,
  workspaceUploadResource,
  workspaceWriteMemory,
} from '../../lib/piClient'
import { AppIcon, type IconName } from '../../components/AppIcon'
import { TeamMembersPanel } from './panels/TeamMembersPanel'
import { TeamResourcesPanel } from './panels/TeamResourcesPanel'
import { TeamMemoryPanel } from './panels/TeamMemoryPanel'
import { TeamArtifactsPanel } from './panels/TeamArtifactsPanel'

export type TeamDrawerTab = 'members' | 'resources' | 'memory' | 'artifacts'

export type TeamDrawerProps = {
  workspace: WorkspaceRecord
  agents: AgentRecord[]
  open: boolean
  activeTab: TeamDrawerTab
  onClose: () => void
  onMembersChanged?: (members: WorkspaceMemberView[]) => void
  onWorkspaceUpdated?: (record: WorkspaceRecord) => void
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(reader.error)
    reader.onload = () => {
      const result = reader.result
      if (typeof result !== 'string') {
        reject(new Error('读取失败'))
        return
      }
      const idx = result.indexOf(',')
      resolve(idx >= 0 ? result.slice(idx + 1) : result)
    }
    reader.readAsDataURL(file)
  })
}

const TABS: { id: TeamDrawerTab; label: string; icon: IconName }[] = [
  { id: 'members', label: '成员', icon: 'users' },
  { id: 'resources', label: '资料', icon: 'folder' },
  { id: 'memory', label: '记忆', icon: 'book' },
  { id: 'artifacts', label: '成果', icon: 'spark' },
]

export function TeamDrawer({
  workspace,
  agents,
  open,
  activeTab,
  onClose,
  onMembersChanged,
  onWorkspaceUpdated,
}: TeamDrawerProps) {
  const drawerTitleTab = TABS.find((t) => t.id === activeTab)
  const [members, setMembers] = useState<WorkspaceMemberView[]>([])
  const [resources, setResources] = useState<WorkspaceResourceRecord[]>([])
  const [memories, setMemories] = useState<WorkspaceMemoryRecord[]>([])
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [peerWorkspaces, setPeerWorkspaces] = useState<{ id: string; name: string }[]>([])
  const [newMemoTitle, setNewMemoTitle] = useState('')
  const [newMemoContent, setNewMemoContent] = useState('')

  const delegateableMemberCount = useMemo(
    () => members.filter((m) => m.agentId !== workspace.supervisorAgentId).length,
    [members, workspace.supervisorAgentId],
  )

  const refresh = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const [mem, res, mm] = await Promise.all([
        workspaceListMembers(workspace.id),
        workspaceListResources(workspace.id),
        workspaceListMemories(workspace.id, 50),
      ])
      setMembers(mem)
      setResources(res)
      setMemories(mm)
      onMembersChanged?.(mem)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [workspace.id, onMembersChanged])

  useEffect(() => {
    void refresh()
  }, [refresh])

  useEffect(() => {
    if (!open || activeTab !== 'members') {
      return
    }
    let cancelled = false
    workspaceList(false)
      .then((list) => {
        if (cancelled) return
        setPeerWorkspaces(
          list
            .filter((w) => w.id !== workspace.id && !w.archived)
            .map((w) => ({ id: w.id, name: w.name })),
        )
      })
      .catch(() => {
        if (!cancelled) setPeerWorkspaces([])
      })
    return () => {
      cancelled = true
    }
  }, [open, activeTab, workspace.id])

  const onAddMembersBatch = async (agentIds: string[]) => {
    setError('')
    try {
      for (const id of agentIds) {
        await workspaceAddMember(workspace.id, id, 'member')
      }
      await refresh()
    } catch (e) {
      setError(String(e))
    }
  }

  const onImportMembersFromWorkspace = async (sourceWorkspaceId: string) => {
    setError('')
    try {
      const srcMembers = await workspaceListMembers(sourceWorkspaceId)
      const current = new Set(members.map((m) => m.agentId))
      const toAdd = srcMembers.map((m) => m.agentId).filter((id) => !current.has(id))
      if (toAdd.length === 0) {
        setError('该工作区没有可新增的成员（均已在本团队）。')
        return
      }
      for (const id of toAdd) {
        await workspaceAddMember(workspace.id, id, 'member')
      }
      await refresh()
    } catch (e) {
      setError(String(e))
    }
  }

  const onRemoveMember = async (agentId: string) => {
    try {
      await workspaceRemoveMember(workspace.id, agentId)
      await refresh()
    } catch (e) {
      setError(String(e))
    }
  }

  const onUpload = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    event.target.value = ''
    if (!file) return
    try {
      const base64 = await fileToBase64(file)
      await workspaceUploadResource({
        workspaceId: workspace.id,
        fileName: file.name,
        dataBase64: base64,
        mime: file.type || null,
      })
      await refresh()
    } catch (e) {
      setError(String(e))
    }
  }

  const onPickFile = (_ref: RefObject<HTMLInputElement | null>) => {
    // TeamResourcesPanel triggers .click() itself after this returns
  }

  const onWriteMemo = async () => {
    const title = newMemoTitle.trim()
    if (!title) return
    try {
      await workspaceWriteMemory({
        workspaceId: workspace.id,
        title,
        content: newMemoContent,
        tags: [],
      })
      setNewMemoTitle('')
      setNewMemoContent('')
      await refresh()
    } catch (e) {
      setError(String(e))
    }
  }

  const onDeleteMemo = async (memoryId: string) => {
    try {
      await workspaceDeleteMemory(workspace.id, memoryId)
      await refresh()
    } catch (e) {
      setError(String(e))
      throw e
    }
  }

  const onDeleteResource = async (resourceId: string) => {
    try {
      await workspaceDeleteResource(workspace.id, resourceId)
      await refresh()
    } catch (e) {
      setError(String(e))
      throw e
    }
  }

  return (
    <aside className={`team-drawer${open ? ' open' : ''}`} aria-hidden={!open}>
      <div className="team-drawer-head">
        <div className="team-drawer-title" aria-live="polite">
          {drawerTitleTab ? (
            <>
              <AppIcon name={drawerTitleTab.icon} size={14} />
              <span>{drawerTitleTab.label}</span>
            </>
          ) : null}
        </div>
        <button
          type="button"
          className="workspace-inline-link team-drawer-close"
          onClick={onClose}
          aria-label="收起侧栏"
        >
          <AppIcon name="close" size={14} />
        </button>
      </div>

      {error ? (
        <div className="skills-feedback error agent-feedback inline team-drawer-error">
          <span>{error}</span>
        </div>
      ) : null}

      <div className="team-drawer-body">
        {activeTab === 'members' ? (
          <TeamMembersPanel
            members={members}
            agents={agents}
            peerWorkspaces={peerWorkspaces}
            onAddMembersBatch={async (ids) => {
              await onAddMembersBatch(ids)
            }}
            onImportMembersFromWorkspace={async (sid) => {
              await onImportMembersFromWorkspace(sid)
            }}
            onRemoveMember={(id) => void onRemoveMember(id)}
          />
        ) : null}
        {activeTab === 'resources' ? (
          <TeamResourcesPanel
            workspace={workspace}
            delegateableMemberCount={delegateableMemberCount}
            onWorkspaceUpdated={onWorkspaceUpdated}
            resources={resources}
            onPickFile={onPickFile}
            onUpload={onUpload}
            onDeleteResource={onDeleteResource}
          />
        ) : null}
        {activeTab === 'memory' ? (
          <TeamMemoryPanel
            memories={memories}
            newMemoTitle={newMemoTitle}
            newMemoContent={newMemoContent}
            onNewMemoTitleChange={setNewMemoTitle}
            onNewMemoContentChange={setNewMemoContent}
            onWriteMemo={() => void onWriteMemo()}
            onDeleteMemory={onDeleteMemo}
            loading={loading}
          />
        ) : null}
        {activeTab === 'artifacts' ? (
          <TeamArtifactsPanel workspace={workspace} onWorkspaceUpdated={onWorkspaceUpdated} onError={setError} />
        ) : null}
      </div>
    </aside>
  )
}

export default TeamDrawer
