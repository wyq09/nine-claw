import { AppIcon } from '../../components/AppIcon'
import type { AgentRecord } from '../../types'
import { getAgentColor } from '../lib'

export type SkillInstallDialogProps = {
  error: string
  link: string
  loading: boolean
  onChangeLink: (value: string) => void
  onClose: () => void
  onConfirm: () => Promise<void>
}

export function SkillInstallDialog({
  error,
  link,
  loading,
  onChangeLink,
  onClose,
  onConfirm,
}: SkillInstallDialogProps) {
  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="confirm-dialog skill-install-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="skill-install-title"
        onClick={(event) => event.stopPropagation()}
      >
        <h3 id="skill-install-title">通过链接安装技能</h3>
        <p>输入技能链接后，会自动打开一个新会话，由助手引导你完成安装与验证。</p>

        <label className="input-field skill-install-field">
          <span>技能链接</span>
          <input
            autoFocus
            value={link}
            onChange={(event) => onChangeLink(event.target.value)}
            placeholder="https://github.com/org/repo 或技能发布页地址"
          />
        </label>

        {error ? <p className="skill-install-error">{error}</p> : null}

        <div className="confirm-dialog-actions">
          <button type="button" className="outline-button" onClick={onClose} disabled={loading}>
            取消
          </button>
          <button type="button" className="outline-button primary" onClick={() => void onConfirm()} disabled={loading}>
            {loading ? '会话启动中…' : '打开安装会话'}
          </button>
        </div>
      </div>
    </div>
  )
}

export type NewSessionDialogProps = {
  agents: AgentRecord[]
  loading: boolean
  modelOptions: { value: string; label: string }[]
  selectedAgentId: string
  selectedModelValue: string
  onChangeAgent: (agentId: string) => void
  onChangeModel: (value: string) => void
  onClose: () => void
  onConfirm: () => void
}

export function NewSessionDialog({
  agents,
  loading,
  modelOptions,
  selectedAgentId,
  selectedModelValue,
  onChangeAgent,
  onChangeModel,
  onClose,
  onConfirm,
}: NewSessionDialogProps) {
  const selectedAgent = agents.find((agent) => agent.id === selectedAgentId) ?? null

  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="confirm-dialog new-session-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="new-session-title"
        onClick={(event) => event.stopPropagation()}
      >
        <h3 id="new-session-title">新会话</h3>
        <p>先选择一个智能体。会话默认使用该智能体挂载的模型，进入聊天后你仍然可以切换模型。</p>

        <div className="new-session-agent-list">
          {agents.length > 0 ? (
            agents.map((agent) => (
              <button
                key={agent.id}
                type="button"
                className={`new-session-agent-card ${agent.id === selectedAgentId ? 'active' : ''}`}
                onClick={() => onChangeAgent(agent.id)}
              >
                <span className="new-session-agent-badge" style={{ backgroundColor: getAgentColor(agent) }}>
                  <AppIcon name="bot" size={18} />
                </span>
                <span className="new-session-agent-copy">
                  <strong>{agent.name}</strong>
                  <span>{agent.summary}</span>
                </span>
              </button>
            ))
          ) : (
            <div className="skill-library-empty compact">
              <strong>{loading ? '智能体加载中…' : '还没有可用智能体'}</strong>
              <p>先去左上角智能体管理里创建或调整一个智能体。</p>
            </div>
          )}
        </div>

        <div className="input-field skill-install-field new-session-model-block">
          <span>本会话模型</span>
          <label className="select-field dialog-select-field">
            <AppIcon name="zap" size={14} />
            <select
              value={selectedModelValue}
              onChange={(event) => onChangeModel(event.target.value)}
              disabled={modelOptions.length === 0}
            >
              {modelOptions.length === 0 ? (
                <option value="">暂无已配置模型</option>
              ) : (
                modelOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))
              )}
            </select>
          </label>
        </div>

        {selectedAgent ? (
          <div className="new-session-tip">
            <strong>当前智能体</strong>
            <span>{selectedAgent.description}</span>
          </div>
        ) : null}

        <div className="confirm-dialog-actions">
          <button type="button" className="outline-button" onClick={onClose}>
            取消
          </button>
          <button
            type="button"
            className="outline-button primary"
            onClick={onConfirm}
            disabled={!selectedAgent}
          >
            进入会话
          </button>
        </div>
      </div>
    </div>
  )
}
