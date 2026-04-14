import { AppIcon } from '../../components/AppIcon'
import type { AgentWorkspaceBundle, AgentWorkspaceFile, BotChannelId, BotConfig } from '../../types'
import type { BotStatusEvent } from '../../lib/piClient'
import { botDefinitions } from '../../mockData'
import { formatWorkspaceFileSectionLabel } from '../lib'

export type AgentBotBindingDialogProps = {
  agentId: string
  agentName: string
  botConfigs: Record<string, BotConfig>
  botLoading: boolean
  botStatusLog: BotStatusEvent[]
  formError: string
  formNotice: string
  onBotConfigChange: (channelId: BotChannelId, updates: Partial<BotConfig>) => void
  onClose: () => void
  onLarkStart: () => void
  onLarkStop: () => void
  onRotatePeerSecret: () => void
  onSave: () => void
  onSelectBot: (id: BotChannelId) => void
  onWechatLogin: () => void
  onWechatStart: () => void
  onWechatStop: () => void
  qrCodeUrl: string
  qrDialogOpen: boolean
  qrStatus: 'waiting' | 'scanned' | 'confirmed' | 'error'
  selectedBotConfig: BotConfig
  selectedBotDefinition: (typeof botDefinitions)[number]
  selectedBotId: BotChannelId
  setBotLoading: (loading: boolean) => void
  setQrDialogOpen: (open: boolean) => void
  saving: boolean
}

export function AgentBotBindingDialog({
  agentId,
  agentName,
  botConfigs,
  botLoading,
  botStatusLog,
  formError,
  formNotice,
  onBotConfigChange,
  onClose,
  onLarkStart,
  onLarkStop,
  onRotatePeerSecret,
  onSave,
  onSelectBot,
  onWechatLogin,
  onWechatStart,
  onWechatStop,
  qrCodeUrl,
  qrDialogOpen,
  qrStatus,
  selectedBotConfig,
  selectedBotDefinition,
  selectedBotId,
  setBotLoading,
  setQrDialogOpen,
  saving,
}: AgentBotBindingDialogProps) {
  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="agent-editor-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-bot-binding-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="agent-editor-dialog-header">
          <div className="agent-editor-header-copy">
            <span className="agent-page-kicker">IM Bot Binding</span>
            <strong id="agent-bot-binding-title">{agentName} 的 IM 机器人绑定</strong>
            <span>独立维护当前智能体的机器人渠道配置，保存后写入数据库。</span>
          </div>

          <button type="button" className="icon-button subtle" onClick={onClose} aria-label="关闭机器人绑定弹窗">
            <AppIcon name="close" size={18} />
          </button>
        </div>

        <div className="agent-editor-dialog-scroll">
          <div className="agent-detail-card agent-editor-card">
            {formError ? (
              <div className="skills-feedback error agent-feedback inline">
                <strong>保存失败</strong>
                <span>{formError}</span>
              </div>
            ) : null}

            {formNotice ? (
              <div className="skills-feedback success agent-feedback inline">
                <strong>已更新</strong>
                <span>{formNotice}</span>
              </div>
            ) : null}

            <div className="bot-settings-layout">
              <div className="bot-channel-list">
                {botDefinitions.map((channel) => {
                  const config = botConfigs[channel.id]
                  return (
                    <button
                      key={channel.id}
                      type="button"
                      className={`bot-channel-card ${selectedBotId === channel.id ? 'active' : ''}`}
                      onClick={() => onSelectBot(channel.id)}
                    >
                      <span className="bot-channel-copy">
                        <strong>{channel.name}</strong>
                        <span
                          className={`bot-status-text ${
                            config.status === '已连接' ? 'connected' : config.status === '错误' ? 'error' : ''
                          }`}
                        >
                          {config.status}
                        </span>
                      </span>
                    </button>
                  )
                })}
              </div>

              <div className="bot-detail-panel">
                <div className="bot-detail-head">
                  <div className="bot-detail-title">
                    <strong className="bot-detail-heading">{selectedBotDefinition.name}</strong>
                    <span
                      className={`bot-status-tag ${
                        selectedBotConfig.status === '已连接'
                          ? 'connected'
                          : selectedBotConfig.status === '错误'
                            ? 'error'
                            : ''
                      }`}
                    >
                      {selectedBotConfig.status}
                    </span>
                  </div>
                </div>

                {selectedBotId === 'peer' ? (
                  <div className="agent-form-grid">
                    <label className="input-field agent-field-full">
                      <span>智能体 ID（对方填 toAgentId）</span>
                      <input readOnly value={agentId} />
                    </label>
                    <label className="input-field agent-field-full">
                      <span>本智能体入站密钥（Bearer）</span>
                      <input
                        value={
                          selectedBotConfig.peerSharedSecret?.trim()
                            ? selectedBotConfig.peerSharedSecret
                            : selectedBotConfig.clientSecret
                        }
                        onChange={(event) =>
                          onBotConfigChange('peer', { peerSharedSecret: event.target.value })
                        }
                        placeholder="保存智能体后由系统自动分配，也可自填"
                        autoComplete="off"
                      />
                    </label>
                    <div className="bot-action-row agent-field-full">
                      <button
                        type="button"
                        className="outline-button"
                        onClick={onRotatePeerSecret}
                        disabled={botLoading || saving}
                      >
                        <span>{botLoading ? '处理中…' : '重新生成密钥'}</span>
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="agent-form-grid">
                    <label className="input-field">
                      <span>{selectedBotDefinition.keyLabel}</span>
                      <input
                        value={selectedBotConfig.clientId}
                        onChange={(event) => onBotConfigChange(selectedBotId, { clientId: event.target.value })}
                        placeholder={selectedBotDefinition.keyPlaceholder}
                      />
                    </label>

                    <label className="input-field">
                      <span>{selectedBotDefinition.secretLabel}</span>
                      <input
                        value={selectedBotConfig.clientSecret}
                        onChange={(event) => onBotConfigChange(selectedBotId, { clientSecret: event.target.value })}
                        placeholder={selectedBotDefinition.secretPlaceholder}
                      />
                    </label>
                  </div>
                )}

                {selectedBotId === 'wechat' ? (
                  <>
                    <label className="input-field agent-field-full">
                      <span>路由标识（可选）</span>
                      <input
                        value={selectedBotConfig.routeTag ?? ''}
                        onChange={(event) => onBotConfigChange('wechat', { routeTag: event.target.value })}
                        placeholder="例如：agent-lawyer"
                      />
                    </label>

                    <div className="bot-action-row">
                      <button type="button" className="outline-button" onClick={onWechatLogin} disabled={botLoading || saving}>
                        <span>{botLoading ? '请稍候...' : '扫码绑定微信'}</span>
                      </button>
                      <button type="button" className="outline-button" onClick={onWechatStart} disabled={botLoading || saving}>
                        <span>{botLoading ? '启动中...' : '启动 Bot'}</span>
                      </button>
                      <button type="button" className="outline-button" onClick={onWechatStop} disabled={botLoading || saving}>
                        <span>{botLoading ? '处理中...' : '断开 Bot'}</span>
                      </button>
                    </div>
                  </>
                ) : selectedBotId === 'lark' ? (
                  <>
                    <div className="agent-workspace-hint">
                      <span>
                        请在飞书开放平台开启机器人能力、长连接事件订阅，并订阅 `im.message.receive_v1`。后台「接收事件」验证前请先在此启动飞书 Bot（保持长连接在线）；若使用 HTTP
                        回调并填内网地址会失败，需公网 URL 或改成长连接。飞书辅助进程与 PI 共用应用内 Node：正式构建经 <code>prepare:pi-runtime</code> 打入的官网完整二进制即可，一般无需再装 Node；若提示「node
                        体积过小」请重新执行 <code>npm run prepare:pi-runtime</code> 后打包。可选环境变量 <code>NINECLAW_LARK_NODE</code> 覆盖路径。
                      </span>
                    </div>
                    <div className="bot-action-row">
                      <button type="button" className="outline-button" onClick={onLarkStart} disabled={botLoading || saving}>
                        <span>{botLoading ? '启动中...' : '启动飞书 Bot'}</span>
                      </button>
                      <button type="button" className="outline-button" onClick={onLarkStop} disabled={botLoading || saving}>
                        <span>{botLoading ? '处理中...' : '断开飞书 Bot'}</span>
                      </button>
                    </div>
                  </>
                ) : selectedBotId === 'peer' ? (
                  <div className="agent-workspace-hint">
                    <span>
                      全应用只需环境变量 <code>NINECLAW_PEER_BIND</code>（如 <code>127.0.0.1:17312</code>）开启监听；<strong>每个智能体各自密钥</strong>鉴权，无全平台共用 Secret。新建或保存智能体会自动补密钥；「重新生成」立即写库。详见{' '}
                      <code>docs/AGENT_PEER_INTEROP.md</code>。
                    </span>
                  </div>
                ) : (
                  <div className="agent-workspace-hint">
                    <span>该渠道当前先支持独立保存绑定信息，运行接入稍后补齐。</span>
                  </div>
                )}

                {selectedBotConfig.errorMessage ? (
                  <div className="skills-feedback error agent-feedback inline">
                    <strong>机器人状态异常</strong>
                    <span>{selectedBotConfig.errorMessage}</span>
                  </div>
                ) : null}

                {botStatusLog.length > 0 ? (
                  <div className="bot-status-log">
                    <strong>最近运行状态</strong>
                    <div className="bot-status-entries">
                      {botStatusLog.slice(0, 6).map((entry, index) => (
                        <div key={`${entry.timestamp}-${index}`} className={`bot-status-entry ${entry.level}`}>
                          <span className="bot-status-level">{entry.level}</span>
                          <span className="bot-status-msg">{entry.message}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                ) : null}
              </div>
            </div>
          </div>
        </div>

        <div className="agent-editor-dialog-footer">
          <button type="button" className="outline-button" onClick={onClose} disabled={saving}>
            关闭
          </button>
          <button type="button" className="outline-button primary" onClick={onSave} disabled={saving}>
            {saving ? '保存中…' : '保存绑定'}
          </button>
        </div>

        {selectedBotId === 'wechat' && qrDialogOpen ? (
          <div className="qr-dialog-overlay" onClick={() => { setQrDialogOpen(false); setBotLoading(false) }}>
            <div className="qr-dialog" onClick={(event) => event.stopPropagation()}>
              <div className="qr-dialog-header">
                <strong>微信扫码绑定</strong>
                <button type="button" className="qr-dialog-close" onClick={() => { setQrDialogOpen(false); setBotLoading(false) }}>
                  &times;
                </button>
              </div>
              <div className="qr-dialog-body">
                {qrStatus === 'waiting' && !qrCodeUrl ? (
                  <div className="qr-loading">正在获取二维码...</div>
                ) : qrStatus === 'waiting' && qrCodeUrl ? (
                  <>
                    <img className="qr-image" src={qrCodeUrl} alt="微信登录二维码" />
                    <p className="qr-hint">请使用微信扫描二维码</p>
                  </>
                ) : qrStatus === 'scanned' ? (
                  <div className="qr-status scanned">
                    <AppIcon name="check" size={48} />
                    <p>已扫描，请在手机上确认</p>
                  </div>
                ) : qrStatus === 'confirmed' ? (
                  <div className="qr-status confirmed">
                    <AppIcon name="check" size={48} />
                    <p>绑定成功</p>
                  </div>
                ) : (
                  <div className="qr-status error">
                    <p>二维码获取失败，请重试</p>
                  </div>
                )}
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  )
}

export type AgentWorkspaceDialogProps = {
  agentName: string
  bundle: AgentWorkspaceBundle | null
  draftContent: string
  error: string
  fileBodyLoading: boolean
  loading: boolean
  onClose: () => void
  onDraftChange: (value: string) => void
  onRefresh: () => void
  onSaveFile: (file: AgentWorkspaceFile, content: string) => void | Promise<void>
  onSelectFile: (key: string) => void | Promise<void>
  saveError: string
  saveLoading: boolean
  saveNotice: string
  selectedFileKey: string
}

export function AgentWorkspaceDialog({
  agentName,
  bundle,
  draftContent,
  error,
  fileBodyLoading,
  loading,
  onClose,
  onDraftChange,
  onRefresh,
  onSaveFile,
  onSelectFile,
  saveError,
  saveLoading,
  saveNotice,
  selectedFileKey,
}: AgentWorkspaceDialogProps) {
  const files = bundle?.files ?? []
  const selectedFile =
    files.find((file) => file.key === selectedFileKey) ??
    files.find((file) => file.exists) ??
    files[0] ??
    null

  const sectionOrder: AgentWorkspaceFile['section'][] = [
    'shared',
    'private',
    'memoryIndex',
    'categoryMemory',
    'dailyLog',
    'wiki',
  ]
  const selectedFileContent = selectedFile?.lazyFetch ? '' : (selectedFile?.content ?? '')
  const selectedFileEditable = Boolean(
    selectedFile && !selectedFile.readOnly && !selectedFile.lazyFetch && !fileBodyLoading,
  )
  const isDirty = Boolean(selectedFile && draftContent !== selectedFileContent)

  const handleResetDraft = () => {
    onDraftChange(selectedFileContent)
  }

  const handleSaveDraft = () => {
    if (!selectedFile || !selectedFileEditable || !isDirty || saveLoading) {
      return
    }

    void onSaveFile(selectedFile, draftContent)
  }

  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="agent-workspace-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-workspace-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="agent-editor-dialog-header">
          <div className="agent-editor-header-copy">
            <span className="agent-page-kicker">Workspace Markdown</span>
            <strong id="agent-workspace-title">{agentName} 的 md 工作区</strong>
            <span>{bundle ? `${bundle.workspaceRoot} · ${bundle.agentHome}` : '读取这个智能体的共享与私有 markdown 文件。'}</span>
          </div>

          <div className="agent-workspace-toolbar">
            <button type="button" className="outline-button" onClick={onRefresh} disabled={loading}>
              <AppIcon name="refresh" size={16} />
              <span>{loading ? '刷新中…' : '刷新'}</span>
            </button>
            <button type="button" className="icon-button subtle" onClick={onClose} aria-label="关闭工作区 Markdown">
              <AppIcon name="close" size={18} />
            </button>
          </div>
        </div>

        <div className="agent-workspace-layout">
          <aside className="agent-workspace-sidebar">
            {sectionOrder.map((section) => {
              const sectionFiles = files.filter((file) => file.section === section)
              if (sectionFiles.length === 0) {
                return null
              }

              return (
                <div key={section} className="agent-workspace-group">
                  <div className="agent-workspace-group-title">{formatWorkspaceFileSectionLabel(section)}</div>
                  <div className="agent-workspace-file-list">
                    {sectionFiles.map((file) => (
                      <button
                        key={file.key}
                        type="button"
                        className={`agent-workspace-file ${selectedFile?.key === file.key ? 'active' : ''}`}
                        onClick={() => void onSelectFile(file.key)}
                      >
                        <strong>{file.name}</strong>
                        <span>{file.relativePath}</span>
                        <small>
                          {file.lazyFetch && file.exists
                            ? '点击加载全文'
                            : file.exists
                              ? '可读取'
                              : '文件不存在'}
                        </small>
                      </button>
                    ))}
                  </div>
                </div>
              )
            })}
          </aside>

          <section className="agent-workspace-content-panel">
            {loading ? (
              <div className="agent-empty-block">
                <strong>正在读取工作区文件…</strong>
                <span>稍等，NineClaw 正在从对应的 agent home 拉取 markdown 内容。</span>
              </div>
            ) : null}

            {!loading && error ? (
              <div className="skills-feedback error agent-feedback inline">
                <strong>读取失败</strong>
                <span>{error}</span>
              </div>
            ) : null}

            {!loading && !error && selectedFile ? (
              <div className="agent-workspace-file-preview">
                <div className="agent-workspace-file-meta">
                  <div>
                    <strong>{selectedFile.name}</strong>
                    <span>{selectedFile.relativePath}</span>
                  </div>
                  <span
                    className={`agent-workspace-status ${
                      selectedFile.readOnly ? 'readonly' : selectedFile.exists ? 'ok' : 'missing'
                    }`}
                  >
                    {fileBodyLoading
                      ? '加载中…'
                      : selectedFile.readOnly
                        ? '只读'
                        : selectedFile.lazyFetch
                          ? '未加载'
                          : selectedFile.exists
                            ? '可编辑'
                            : '保存后创建'}
                  </span>
                </div>

                {fileBodyLoading ? (
                  <div className="agent-empty-block">
                    <strong>正在读取文件内容…</strong>
                  </div>
                ) : null}

                {!fileBodyLoading && selectedFile.readOnly ? (
                  <div className="agent-workspace-hint">
                    <strong>这个文件由系统运行时维护。</strong>
                    <span>当前只提供查看，不支持从配置页直接覆盖写入。</span>
                  </div>
                ) : null}

                <div className="agent-workspace-editor-actions">
                  <span className="agent-workspace-editor-hint">
                    {selectedFile.readOnly
                      ? '只读文件'
                      : selectedFile.scope === 'shared'
                        ? '共享文件，保存后会影响所有智能体'
                      : selectedFile.exists
                        ? '可直接编辑并保存回 workspace'
                        : '这个文件当前不存在，保存后会自动创建'}
                  </span>
                  <div className="confirm-dialog-actions agent-workspace-editor-buttons">
                    <button
                      type="button"
                      className="outline-button"
                      onClick={handleResetDraft}
                      disabled={!selectedFileEditable || !isDirty || saveLoading}
                    >
                      重置
                    </button>
                    <button
                      type="button"
                      className="primary-button"
                      onClick={handleSaveDraft}
                      disabled={!selectedFileEditable || !isDirty || saveLoading}
                    >
                      {saveLoading ? '保存中…' : '保存'}
                    </button>
                  </div>
                </div>

                {saveError ? (
                  <div className="skills-feedback error agent-feedback inline">
                    <strong>保存失败</strong>
                    <span>{saveError}</span>
                  </div>
                ) : null}

                {saveNotice ? (
                  <div className="skills-feedback success agent-feedback inline">
                    <strong>保存成功</strong>
                    <span>{saveNotice}</span>
                  </div>
                ) : null}

                <textarea
                  className="agent-workspace-content agent-workspace-editor"
                  value={draftContent}
                  onChange={(event) => onDraftChange(event.target.value)}
                  placeholder={selectedFile.exists ? '' : '# 新文件\n\n在这里输入要保存的 markdown 内容。'}
                  readOnly={!selectedFileEditable}
                  spellCheck={false}
                />
              </div>
            ) : null}

            {!loading && !error && !selectedFile ? (
              <div className="agent-empty-block">
                <strong>没有可展示的文件</strong>
                <span>这个智能体还没有生成任何 markdown 文件。</span>
              </div>
            ) : null}
          </section>
        </div>
      </div>
    </div>
  )
}
