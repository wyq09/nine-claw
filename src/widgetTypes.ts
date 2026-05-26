export type WidgetStatus = 'pending' | 'submitted' | 'cancelled' | 'expired'

export type WidgetOption = {
  id: string
  label: string
  description?: string
}

type AskUserQuestionBase = {
  id: string
  label: string
  description?: string
  required?: boolean
}

export type AskUserTextQuestion = AskUserQuestionBase & {
  type: 'text' | 'textarea'
  placeholder?: string
  maxLength?: number
}

export type AskUserChoiceQuestion = AskUserQuestionBase & {
  type: 'single_select' | 'multi_select'
  options: WidgetOption[]
  recommendedOptionId?: string
  allowCustomInput?: boolean
  customInputPlaceholder?: string
  minSelections?: number
  maxSelections?: number
}

export type AskUserQuestion = AskUserTextQuestion | AskUserChoiceQuestion

export type AskUserAnswerValue = string | string[]

export type AskUserAnswerDraft = {
  questionId: string
  value: AskUserAnswerValue
  customValue?: string
}

export type AskUserWidget = {
  kind: 'ask_user'
  widgetId: string
  version: number
  title: string
  description?: string
  submitLabel?: string
  cancelLabel?: string
  allowSkip?: boolean
  status: WidgetStatus
  questions: AskUserQuestion[]
}

export type WidgetDefinition = AskUserWidget

export type WidgetSegment = {
  type: 'widget'
  widget: WidgetDefinition
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function isWidgetOption(value: unknown): value is WidgetOption {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    value.id.trim().length > 0 &&
    typeof value.label === 'string' &&
    value.label.trim().length > 0 &&
    (value.description === undefined || typeof value.description === 'string')
  )
}

function isChoiceQuestion(question: AskUserQuestion): question is AskUserChoiceQuestion {
  return question.type === 'single_select' || question.type === 'multi_select'
}

function parseAskUserQuestion(value: unknown): AskUserQuestion | null {
  if (!isRecord(value) || typeof value.id !== 'string' || typeof value.label !== 'string') {
    return null
  }

  const base = {
    id: value.id,
    label: value.label,
    ...(typeof value.description === 'string' ? { description: value.description } : {}),
    ...(typeof value.required === 'boolean' ? { required: value.required } : {}),
  }

  if (value.type === 'text' || value.type === 'textarea') {
    return {
      ...base,
      type: value.type,
      ...(typeof value.placeholder === 'string' ? { placeholder: value.placeholder } : {}),
      ...(typeof value.maxLength === 'number' ? { maxLength: value.maxLength } : {}),
    }
  }

  if (value.type === 'single_select' || value.type === 'multi_select') {
    if (!Array.isArray(value.options)) {
      return null
    }
    const options = value.options.filter(isWidgetOption)
    if (options.length === 0) {
      return null
    }
    return {
      ...base,
      type: value.type,
      options,
      ...(typeof value.recommendedOptionId === 'string'
        ? { recommendedOptionId: value.recommendedOptionId }
        : {}),
      ...(typeof value.allowCustomInput === 'boolean'
        ? { allowCustomInput: value.allowCustomInput }
        : {}),
      ...(typeof value.customInputPlaceholder === 'string'
        ? { customInputPlaceholder: value.customInputPlaceholder }
        : {}),
      ...(typeof value.minSelections === 'number' ? { minSelections: value.minSelections } : {}),
      ...(typeof value.maxSelections === 'number' ? { maxSelections: value.maxSelections } : {}),
    }
  }

  return null
}

export function parseWidgetSegment(value: unknown): WidgetSegment | null {
  if (!isRecord(value) || value.type !== 'widget' || !isRecord(value.widget)) {
    return null
  }

  const widget = value.widget
  if (
    widget.kind !== 'ask_user' ||
    typeof widget.widgetId !== 'string' ||
    typeof widget.version !== 'number' ||
    typeof widget.title !== 'string' ||
    !Array.isArray(widget.questions)
  ) {
    return null
  }

  const questions = widget.questions
    .map((item) => parseAskUserQuestion(item))
    .filter((item): item is AskUserQuestion => item !== null)
  if (questions.length === 0) {
    return null
  }

  const status: WidgetStatus =
    widget.status === 'submitted' ||
    widget.status === 'cancelled' ||
    widget.status === 'expired'
      ? widget.status
      : 'pending'

  return {
    type: 'widget',
    widget: {
      kind: 'ask_user',
      widgetId: widget.widgetId,
      version: widget.version,
      title: widget.title,
      ...(typeof widget.description === 'string' ? { description: widget.description } : {}),
      ...(typeof widget.submitLabel === 'string' ? { submitLabel: widget.submitLabel } : {}),
      ...(typeof widget.cancelLabel === 'string' ? { cancelLabel: widget.cancelLabel } : {}),
      ...(typeof widget.allowSkip === 'boolean' ? { allowSkip: widget.allowSkip } : {}),
      status,
      questions,
    },
  }
}

/**
 * ask_user 是 widget 平台里的一个严格子集。
 * 它比通用表单更强约束，目的是让模型在信息缺失时“不猜，直接问”。
 */
export function validateAskUserToolPolicy(widget: AskUserWidget): string[] {
  const errors: string[] = []

  if (widget.questions.length !== 1) {
    errors.push('ask_user 每次只能问一个问题。')
  }

  for (const question of widget.questions) {
    if (!isChoiceQuestion(question)) {
      continue
    }

    if (question.options.length < 2 || question.options.length > 6) {
      errors.push('ask_user 选项数量必须在 2 到 6 个之间。')
    }

    const firstOption = question.options[0]
    if (!firstOption) {
      errors.push('ask_user 需要至少一个推荐选项。')
    } else if (question.recommendedOptionId !== firstOption.id) {
      errors.push('ask_user 的第一个选项必须是推荐选项。')
    }

    const lastOption = question.options[question.options.length - 1]
    if (!lastOption || lastOption.label.trim() !== '其他') {
      errors.push('ask_user 的最后一个选项必须是“其他”。')
    }
  }

  return errors
}
