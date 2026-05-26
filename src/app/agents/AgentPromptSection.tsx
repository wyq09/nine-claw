import { useState } from 'react'
import { createPortal } from 'react-dom'
import { AppIcon } from '../../components/AppIcon'

type AgentPromptSectionProps = {
  value: string
  onChange: (value: string) => void
}

export function AgentPromptSection({ value, onChange }: AgentPromptSectionProps) {
  const [promptEditorOpen, setPromptEditorOpen] = useState(false)

  return (
    <>
      <div className="agent-section">
        <div className="agent-section-header">
          <div>
            <strong>提示词内容</strong>
            <p>可使用 {'${ARG}'} 占位符引用本次手动触发或自动调用时的用户输入。</p>
          </div>
          <button
            type="button"
            className="outline-button"
            onClick={() => setPromptEditorOpen(true)}
            title="放大编辑提示词"
          >
            <AppIcon name="eye" size={16} />
            <span>放大编辑</span>
          </button>
        </div>

        <label className="input-field agent-field-full">
          <span>提示词内容</span>
          <textarea
            value={value}
            onChange={(event) => onChange(event.target.value)}
            rows={8}
            placeholder="例如：你是合同审查智能体。请围绕 ${ARG} 输出风险点、修改建议和需要用户补充的信息。"
          />
        </label>
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
                    value={value}
                    onChange={(event) => onChange(event.target.value)}
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
    </>
  )
}
