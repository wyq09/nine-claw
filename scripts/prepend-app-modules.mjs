import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

const chatWorkspaceHeader = `import type { ChangeEvent, ClipboardEvent, KeyboardEvent, MutableRefObject, PointerEvent as ReactPointerEvent, RefObject } from 'react'
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { AppIcon, type IconName } from '../../components/AppIcon'
import { ComposerAttachmentStrip } from '../../components/ComposerAttachmentStrip'
import { PromptBubbleContent } from '../../components/PromptBubbleContent'
import type { AgentBuilderDraft, ConversationAgentSnapshot, HistoryItem, PersistedChatAttachment, SubmitShortcut } from '../../types'
import {
  clampNumber,
  DEFAULT_COMPOSER_HEIGHT,
  getSubmitShortcutLabel,
  MAX_COMPOSER_HEIGHT,
  MIN_COMPOSER_HEIGHT,
  parseAgentBuilderDraft,
  shouldSubmitWithShortcut,
  STARTER_CHIPS,
  stripAgentBuilderBlock,
  TokenUsageDetailPill,
} from '../lib'
import { ImagePreviewModal, TurnExecutionDetails, TurnResponseBody } from './TurnAndTools'

`

const turnAndToolsHeader = `import { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import type { MouseEvent } from 'react'
import { Check, Copy } from 'lucide-react'
import { AppIcon } from '../../components/AppIcon'
import { InlineMediaAttachmentList } from '../../components/InlineMediaAttachmentList'
import ReplyCardStack from '../../components/ReplyCardStack'
import { extractInlineMediaAttachments, normalizeMarkdownImageSources } from '../../lib/inlineMedia'
import { openExternalUrl } from '../../lib/piClient'
import { resolveReplyCardItems } from '../../lib/replyCardFormat'
import type { AgentBuilderDraft, ConversationTurn, ToolCallEntry } from '../../types'
import {
  formatAgentExecutionModeLabel,
  formatDurationLabel,
  parseAgentBuilderDraft,
  stripAgentBuilderBlock,
  TURN_PLACEHOLDER_NO_OUTPUT,
  useLiveNow,
} from '../lib'

const MarkdownRenderer = lazy(() => import('../../components/MarkdownRenderer'))

`

const libraryAndTasksHeader = `import type { Dispatch, SetStateAction } from 'react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { AppIcon } from '../../components/AppIcon'
import { useToast } from '../../hooks/useToast'
import type { AgentRecord, AgentTaskListItem, AgentTaskUpdateInput, InstalledSkillItem, ResourceItem, SkillLibraryTab, SystemSkillCatalog } from '../../types'
import {
  deleteAgentTask,
  listAgentTasks,
  pauseAgentTask,
  resumeAgentTask,
  runAgentTaskNow,
  updateAgentTask,
} from '../../lib/piClient'
import {
  formatAgentTaskScheduleShort,
  formatAgentTaskStatus,
  formatInstalledSkillScopeLabel,
  formatInstalledSkillSource,
  formatOptionalAbsoluteTime,
  getAgentColor,
  groupAgentTasksByStatus,
  parseDailyTimesInput,
  runAtMsToDatetimeLocalValue,
  SkillDescriptionDisclosure,
} from '../lib'

`

const agentDialogsBundleHeader = `import { useCallback, useEffect, useMemo, useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { AppIcon } from '../../components/AppIcon'
import type { AgentInput, AgentRecord, InstalledSkillItem, PeerGatewayInfo, ProviderId } from '../../types'
import { getPeerGatewayInfo } from '../../lib/piClient'
import {
  createEmptyAgentDraft,
  createAgentDraftFromRecord,
  formatAgentExecutionModeLabel,
  formatInstalledSkillScopeLabel,
  formatInstalledSkillSource,
  formatWorkspaceFileSectionLabel,
  normalizeAgentDraft,
  SkillDescriptionDisclosure,
  validateAgentDraft,
} from '../lib'

`

const agentChannelDialogsHeader = `import { AppIcon } from '../../components/AppIcon'
import type { BotChannelId, BotConfig } from '../../types'
import type { BotStatusEvent } from '../../lib/piClient'
import { botDefinitions } from '../../mockData'

`

const agentsViewHeader = `import { AppIcon } from '../../components/AppIcon'
import type {
  AgentInput,
  AgentRecord,
  AgentWorkspaceBundle,
  AgentWorkspaceFile,
  BotChannelId,
  BotConfig,
  InstalledSkillItem,
} from '../../types'
import type { BotStatusEvent } from '../../lib/piClient'
import { botDefinitions } from '../../mockData'
import { getAgentColor } from '../lib'
import { AgentEditorDialog } from './AgentDialogsBundle'
import { AgentBotBindingDialog, AgentWorkspaceDialog } from './AgentChannelDialogs'

`

function prepend(rel, header) {
  const p = path.join(root, rel)
  const body = fs.readFileSync(p, 'utf8')
  if (body.startsWith('import ')) {
    return
  }
  fs.writeFileSync(p, header + body, 'utf8')
}

prepend('src/app/chat/ChatWorkspace.tsx', chatWorkspaceHeader)
prepend('src/app/chat/TurnAndTools.tsx', turnAndToolsHeader)
prepend('src/app/pages/LibraryAndTasks.tsx', libraryAndTasksHeader)
prepend('src/app/agents/AgentDialogsBundle.tsx', agentDialogsBundleHeader)
prepend('src/app/agents/AgentChannelDialogs.tsx', agentChannelDialogsHeader)
prepend('src/app/agents/AgentsView.tsx', agentsViewHeader)

console.log('prepended headers')
