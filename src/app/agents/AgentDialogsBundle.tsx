import { open } from '@tauri-apps/plugin-dialog'
import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { AppIcon } from '../../components/AppIcon'
import { AgentAvatar } from '../../components/AgentAvatar'
import type {
  AgentInput,
  AgentRecord,
  InstalledSkillItem,
} from '../../types'
import {
  formatInstalledSkillScopeLabel,
  formatInstalledSkillSource,
  getAgentColor,
  SkillDescriptionDisclosure,
} from '../lib'
import { AgentEditorConfiguration } from './AgentEditorConfiguration'

export type AgentSkillPickerDialogProps = {
  allSkillCount: number
  searchValue: string
  selectedSkillIds: string[]
  skills: InstalledSkillItem[]
  onClose: () => void
  onSearch: (value: string) => void
  onToggleSkill: (skillId: string) => void
}

export function AgentSkillPickerDialog({
  allSkillCount,
  searchValue,
  selectedSkillIds,
  skills,
  onClose,
  onSearch,
  onToggleSkill,
}: AgentSkillPickerDialogProps) {
  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="confirm-dialog agent-skill-picker-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-skill-picker-title"
        onClick={(event) => event.stopPropagation()}
      >
        <h3 id="agent-skill-picker-title">添加允许技能</h3>
        <p>从本地已安装技能里搜索并选择。选中的技能会成为这个智能体可加载的技能集合。</p>

        <label className="input-field skill-install-field">
          <span>搜索技能</span>
          <input
            autoFocus
            value={searchValue}
            onChange={(event) => onSearch(event.target.value)}
            placeholder="搜索技能名称、说明、来源或路径"
          />
        </label>

        <div className="agent-skill-picker-meta">
          <span>已安装 {allSkillCount} 个</span>
          <span>已选中 {selectedSkillIds.length} 个</span>
        </div>

        <div className="agent-skill-picker-list">
          {skills.length > 0 ? (
            skills.map((skill) => {
              const active = selectedSkillIds.includes(skill.id)
              return (
                <article
                  key={skill.id}
                  className={`agent-skill-option ${active ? 'active' : ''}`}
                >
                  <div className="agent-skill-option-copy">
                    <strong>{skill.name}</strong>
                    <SkillDescriptionDisclosure description={skill.description} className="skill-description-inset" />
                    <small>
                      {formatInstalledSkillScopeLabel(skill.scope)} · {formatInstalledSkillSource(skill)}
                    </small>
                  </div>
                  <div className="agent-skill-option-actions">
                    <button
                      type="button"
                      className="agent-skill-option-action"
                      onClick={() => onToggleSkill(skill.id)}
                    >
                      {active ? '已添加' : '添加技能'}
                    </button>
                  </div>
                </article>
              )
            })
          ) : (
            <div className="agent-skill-picker-empty">
              <strong>{allSkillCount > 0 ? '没有匹配的技能' : '暂无已安装技能'}</strong>
              <span>
                {allSkillCount > 0 ? '换个关键词继续搜索，或直接关闭弹窗。' : '先去技能库安装技能，再回到这里选择偏好技能。'}
              </span>
            </div>
          )}
        </div>

        <div className="confirm-dialog-actions">
          <button type="button" className="outline-button" onClick={onClose}>
            完成
          </button>
        </div>
      </div>
    </div>
  )
}

export type AgentEditorDialogProps = {
  agentDraft: AgentInput | null
  agentDeleteConfirmOpen: boolean
  agentDeleteConfirmText: string
  agentFormError: string
  agentFormNotice: string
  agentRefreshing: boolean
  agentSaving: boolean
  allSkills: InstalledSkillItem[]
  defaultAgentId: string
  mode: 'create' | 'edit'
  modelOptions: { value: string; label: string }[]
  onClose: () => void
  onCloseDeleteAgentDialog: () => void
  onConfirmDeleteAgent: () => void
  onCreateAgent: () => void
  onDraftChange: (updates: Partial<AgentInput>) => void
  onDeleteConfirmTextChange: (value: string) => void
  onRefreshAgent: () => void
  onOpenWorkspace: () => void
  onOpenSkillPicker: () => void
  onRequestDeleteAgent: () => void
  onSaveAgent: () => void
  onSetDefaultAgent: () => void
  onToggleSkill: (skillId: string) => void
  selectedAgent: AgentRecord | null
  /** 已保存智能体的 id；新建为空字符串 */
  managedAgentId: string
}

export function AgentEditorDialog({
  agentDraft,
  agentDeleteConfirmOpen,
  agentDeleteConfirmText,
  agentFormError,
  agentFormNotice,
  agentRefreshing,
  agentSaving,
  allSkills,
  defaultAgentId,
  mode,
  modelOptions,
  onClose,
  onCloseDeleteAgentDialog,
  onConfirmDeleteAgent,
  onCreateAgent,
  onDraftChange,
  onDeleteConfirmTextChange,
  onRefreshAgent,
  onOpenWorkspace,
  onOpenSkillPicker,
  onRequestDeleteAgent,
  onSaveAgent,
  onSetDefaultAgent,
  onToggleSkill,
  selectedAgent,
  managedAgentId,
}: AgentEditorDialogProps) {
  const [promptEditorOpen, setPromptEditorOpen] = useState(false)
  const [avatarMenu, setAvatarMenu] = useState<null | { x: number; y: number; panel: 'main' | 'url' }>(null)
  const [avatarUrlDraft, setAvatarUrlDraft] = useState('')
  const avatarMenuButtonRef = useRef<HTMLButtonElement | null>(null)
  const avatarUrlInputRef = useRef<HTMLInputElement | null>(null)

  useEffect(() => {
    if (!avatarMenu || avatarMenu.panel !== 'url') {
      return
    }
    const id = requestAnimationFrame(() => avatarUrlInputRef.current?.focus())
    return () => cancelAnimationFrame(id)
  }, [avatarMenu])

  useEffect(() => {
    if (!avatarMenu) {
      return
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation()
        setAvatarMenu(null)
      }
    }
    const onScroll = () => setAvatarMenu(null)
    document.addEventListener('keydown', onKey, true)
    window.addEventListener('scroll', onScroll, true)
    return () => {
      document.removeEventListener('keydown', onKey, true)
      window.removeEventListener('scroll', onScroll, true)
    }
  }, [avatarMenu])

  const editorAccent = getAgentColor(
    selectedAgent ?? { id: 'draft', name: agentDraft?.name || '智能体', accentColor: agentDraft?.accentColor },
  )
  const displayAgentId = agentDraft?.id?.trim() || managedAgentId.trim()

  if (!agentDraft) {
    return null
  }

  const handlePickAvatar = async (): Promise<boolean> => {
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: '头像图片', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp', 'svg'] }],
    })
    const filePath = Array.isArray(picked) ? picked[0] : picked
    if (typeof filePath === 'string' && filePath.trim()) {
      onDraftChange({ avatarUri: filePath })
      return true
    }
    return false
  }

  const onAvatarButtonClick = () => {
    if (avatarMenu) {
      setAvatarMenu(null)
      return
    }
    const el = avatarMenuButtonRef.current
    if (!el) {
      return
    }
    setAvatarUrlDraft(agentDraft.avatarUri?.trim() ?? '')
    const r = el.getBoundingClientRect()
    setAvatarMenu({ x: r.left, y: r.bottom + 6, panel: 'main' })
  }

  const pickAvatarFromMenu = async () => {
    const ok = await handlePickAvatar()
    if (ok) {
      setAvatarMenu(null)
    }
  }

  const openUrlPanel = () => {
    setAvatarUrlDraft(agentDraft.avatarUri?.trim() ?? '')
    setAvatarMenu((m) => (m ? { ...m, panel: 'url' } : m))
  }

  const backToMainPanel = () => {
    setAvatarMenu((m) => (m ? { ...m, panel: 'main' } : m))
  }

  const applyAvatarUrlFromMenu = () => {
    onDraftChange({ avatarUri: avatarUrlDraft.trim() })
    setAvatarMenu(null)
  }

  const clearAvatarFromMenu = () => {
    onDraftChange({ avatarUri: '' })
    setAvatarMenu(null)
  }

  return (
    <div className="agent-editor-page-root">
      <div
        className="agent-editor-dialog agent-editor-page"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-editor-title"
      >
        <header className="agent-editor-hero-band">
          <div className="agent-detail-hero agent-editor-hero-stage" style={{ borderColor: editorAccent + '1f' }}>
            <div className="agent-editor-hero-head">
              <div className="agent-detail-hero-main">
                <div className="agent-editor-hero-avatar">
                  <button
                    type="button"
                    ref={avatarMenuButtonRef}
                    className="agent-editor-hero-avatar-pick"
                    onClick={onAvatarButtonClick}
                    title="设置头像"
                    aria-label="设置头像"
                    aria-haspopup="menu"
                    aria-expanded={Boolean(avatarMenu)}
                  >
                    <AgentAvatar
                      name={selectedAgent?.name ?? agentDraft.name ?? '智能体'}
                      avatarUri={agentDraft.avatarUri}
                      accentColor={editorAccent}
                      className="agent-badge large"
                      size={24}
                      fallbackToIcon={!agentDraft.avatarUri}
                    />
                  </button>
                </div>
                <div className="agent-detail-copy">
                  <span className="agent-page-kicker">Agent Studio</span>
                  <h2 id="agent-editor-title">{mode === 'create' ? '新建智能体' : selectedAgent?.name ?? '编辑智能体'}</h2>
                  <p className="agent-editor-hero-summary">
                    {agentDraft.summary.trim() || agentDraft.description.trim() || '配置名称、模型、工具、技能和提示词。'}
                  </p>
                </div>
              </div>
              <div className="agent-editor-hero-toolbar">
                <button type="button" className="outline-button" onClick={onClose}>
                  <AppIcon name="arrow-left" size={16} />
                  <span>返回列表</span>
                </button>
                <button
                  type="button"
                  className="outline-button"
                  onClick={onRefreshAgent}
                  disabled={agentRefreshing || agentSaving || mode !== 'edit' || !selectedAgent}
                >
                  <AppIcon name="refresh" size={16} />
                  <span>{agentRefreshing ? '刷新中…' : '刷新'}</span>
                </button>
                <button
                  type="button"
                  className="outline-button"
                  onClick={onOpenWorkspace}
                  disabled={mode !== 'edit' || !selectedAgent}
                >
                  <AppIcon name="folder" size={16} />
                  <span>工作区</span>
                </button>
                <button
                  type="button"
                  className="outline-button"
                  onClick={onOpenSkillPicker}
                  disabled={allSkills.length === 0}
                >
                  <AppIcon name="puzzle" size={16} />
                  <span>技能库</span>
                </button>
                <button type="button" className="icon-button subtle" onClick={onClose} aria-label="关闭智能体配置页">
                  <AppIcon name="close" size={18} />
                </button>
              </div>
            </div>
            <div className="agent-hero-pills">
              <span className="agent-hero-pill">{mode === 'create' ? '未保存' : '用户智能体'}</span>
              <span className="agent-hero-pill">{agentDraft.allowedToolIds.length} 个工具</span>
              <span className="agent-hero-pill">{agentDraft.skillIds.length} 个技能</span>
              {displayAgentId ? <span className="agent-hero-pill">ID: {displayAgentId}</span> : null}
              {selectedAgent?.id === defaultAgentId ? <span className="agent-hero-pill accent">当前默认</span> : null}
            </div>
          </div>
        </header>

        <div className="agent-editor-dialog-scroll">
          <div className="agent-editor-form-surface">
            {agentFormError ? (
              <div className="skills-feedback error agent-feedback inline agent-editor-banner">
                <strong>保存失败</strong>
                <span>{agentFormError}</span>
              </div>
            ) : null}

            {agentFormNotice ? (
              <div className="skills-feedback success agent-feedback inline agent-editor-banner">
                <strong>已更新</strong>
                <span>{agentFormNotice}</span>
              </div>
            ) : null}

            <AgentEditorConfiguration
              agentDraft={agentDraft}
              allSkills={allSkills}
              mode={mode}
              modelOptions={modelOptions}
              onDraftChange={onDraftChange}
              onOpenPromptEditor={() => setPromptEditorOpen(true)}
              onOpenSkillPicker={onOpenSkillPicker}
              onToggleSkill={onToggleSkill}
              selectedAgent={selectedAgent}
            />
          </div>
        </div>

        <div className="agent-editor-dialog-footer">
          <button type="button" className="outline-button" onClick={onCreateAgent} disabled={agentSaving}>
            重新新建
          </button>
          {selectedAgent && mode === 'edit' ? (
            <button
              type="button"
              className="outline-button"
              onClick={onSetDefaultAgent}
              disabled={agentSaving || selectedAgent.id === defaultAgentId}
            >
              {selectedAgent.id === defaultAgentId ? '当前默认' : '设为默认'}
            </button>
          ) : null}
          {selectedAgent && mode === 'edit' ? (
            <button type="button" className="outline-button danger" onClick={onRequestDeleteAgent} disabled={agentSaving}>
              删除
            </button>
          ) : null}
          <button type="button" className="outline-button primary" onClick={onSaveAgent} disabled={agentSaving}>
            {agentSaving ? '保存中…' : mode === 'create' ? '创建智能体' : '保存修改'}
          </button>
        </div>

        {agentDeleteConfirmOpen && selectedAgent ? (
          <div className="confirm-dialog-overlay" role="presentation" onClick={onCloseDeleteAgentDialog}>
            <div className="confirm-dialog" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
              <h3>删除智能体</h3>
              <p>删除后会一并清除这个智能体的数据库记录、绑定机器人数据和 md 工作区。</p>
              <p>请输入“确认删除”以删除「{selectedAgent.name}」。</p>
              <label className="input-field">
                <span>确认口令</span>
                <input
                  value={agentDeleteConfirmText}
                  onChange={(event) => onDeleteConfirmTextChange(event.target.value)}
                  placeholder="确认删除"
                />
              </label>
              <div className="confirm-dialog-actions">
                <button type="button" className="outline-button" onClick={onCloseDeleteAgentDialog} disabled={agentSaving}>
                  取消
                </button>
                <button
                  type="button"
                  className="outline-button confirm-dialog-delete"
                  onClick={onConfirmDeleteAgent}
                  disabled={agentSaving || agentDeleteConfirmText.trim() !== '确认删除'}
                >
                  {agentSaving ? '删除中…' : '确认删除'}
                </button>
              </div>
            </div>
          </div>
        ) : null}
      </div>
      {promptEditorOpen
        ? createPortal(
            <div className="confirm-dialog-overlay agent-prompt-editor-overlay" role="presentation" onClick={() => setPromptEditorOpen(false)}>
              <div
                className="confirm-dialog agent-prompt-editor-dialog"
                role="dialog"
                aria-modal="true"
                aria-labelledby="agent-prompt-editor-title"
                onClick={(event) => event.stopPropagation()}
              >
                <div className="agent-prompt-editor-head">
                  <div>
                    <strong id="agent-prompt-editor-title">大提示词编辑器</strong>
                    <p>这里编辑的内容会直接保存到当前智能体的系统提示词字段。</p>
                  </div>
                  <button
                    type="button"
                    className="icon-button subtle"
                    onClick={() => setPromptEditorOpen(false)}
                    aria-label="关闭提示词放大编辑器"
                  >
                    <AppIcon name="close" size={18} />
                  </button>
                </div>
                <label className="input-field agent-field-full agent-prompt-editor-field">
                  <span>提示词内容</span>
                  <textarea
                    className="agent-prompt-editor-textarea"
                    autoFocus
                    value={agentDraft.systemPrompt}
                    onChange={(event) => onDraftChange({ systemPrompt: event.target.value })}
                    rows={20}
                    placeholder="例如：你是合同审查智能体。请围绕 ${ARG} 输出风险点、修改建议和需要用户补充的信息。"
                  />
                </label>
                <div className="confirm-dialog-actions">
                  <button type="button" className="outline-button" onClick={() => setPromptEditorOpen(false)}>
                    完成
                  </button>
                </div>
              </div>
            </div>,
            document.body,
          )
        : null}

      {avatarMenu
        ? createPortal(
            <>
              <div
                role="presentation"
                className="agent-editor-avatar-menu-backdrop"
                onClick={() => setAvatarMenu(null)}
              />
              <div
                className="agent-editor-avatar-menu"
                style={{ left: avatarMenu.x, top: avatarMenu.y }}
                role="menu"
                onMouseDown={(event) => event.stopPropagation()}
              >
                {avatarMenu.panel === 'main' ? (
                  <>
                    <button
                      type="button"
                      role="menuitem"
                      className="agent-editor-avatar-menu-item"
                      onClick={() => void pickAvatarFromMenu()}
                    >
                      <AppIcon name="upload" size={16} />
                      <span>选择本地图片</span>
                    </button>
                    <button type="button" role="menuitem" className="agent-editor-avatar-menu-item" onClick={openUrlPanel}>
                      <AppIcon name="network" size={16} />
                      <span>输入或粘贴地址</span>
                    </button>
                    <div className="agent-editor-avatar-menu-sep" role="separator" />
                    <button
                      type="button"
                      role="menuitem"
                      className="agent-editor-avatar-menu-item danger"
                      disabled={!agentDraft.avatarUri?.trim()}
                      onClick={clearAvatarFromMenu}
                    >
                      <AppIcon name="trash" size={16} />
                      <span>清空头像</span>
                    </button>
                  </>
                ) : (
                  <div className="agent-editor-avatar-menu-url" role="none">
                    <label className="agent-editor-avatar-menu-url-label">
                      <span>路径或 URL</span>
                      <input
                        ref={avatarUrlInputRef}
                        value={avatarUrlDraft}
                        onChange={(event) => setAvatarUrlDraft(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter') {
                            event.preventDefault()
                            applyAvatarUrlFromMenu()
                          }
                        }}
                        placeholder="本地路径、https://、file://、asset:"
                      />
                    </label>
                    <div className="agent-editor-avatar-menu-url-row">
                      <button type="button" className="outline-button" onClick={backToMainPanel}>
                        返回
                      </button>
                      <button type="button" className="primary-button" onClick={applyAvatarUrlFromMenu}>
                        应用
                      </button>
                    </div>
                  </div>
                )}
              </div>
            </>,
            document.body,
          )
        : null}
    </div>
  )
}
