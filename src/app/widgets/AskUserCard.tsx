import { useMemo, useState } from 'react'
import type { AskUserAnswerDraft, AskUserChoiceQuestion, AskUserQuestion, AskUserWidget } from '../../widgetTypes'

export type AskUserCardProps = {
  widget: AskUserWidget
  onSubmit?: (payload: {
    widgetId: string
    kind: 'ask_user'
    answers: AskUserAnswerDraft[]
  }) => Promise<void> | void
  onCancel?: (payload: { widgetId: string; kind: 'ask_user' }) => Promise<void> | void
}

function buildInitialAnswers(questions: AskUserQuestion[]): Record<string, AskUserAnswerDraft> {
  return Object.fromEntries(
    questions.map((question) => [
      question.id,
      {
        questionId: question.id,
        value: question.type === 'multi_select' ? [] : '',
        customValue: '',
      },
    ]),
  )
}

function questionHasAnswer(question: AskUserQuestion, draft: AskUserAnswerDraft | undefined): boolean {
  if (!draft) {
    return false
  }
  if (question.type === 'text' || question.type === 'textarea') {
    return typeof draft.value === 'string' && draft.value.trim().length > 0
  }
  const selected = Array.isArray(draft.value) ? draft.value : draft.value ? [draft.value] : []
  const hasCustom = Boolean(draft.customValue?.trim())
  return selected.length > 0 || hasCustom
}

function validateQuestion(question: AskUserQuestion, draft: AskUserAnswerDraft | undefined): string | null {
  if (!question.required) {
    return null
  }
  return questionHasAnswer(question, draft) ? null : '此项必填'
}

function updateChoiceSelection(
  question: AskUserChoiceQuestion,
  current: AskUserAnswerDraft,
  optionId: string,
  checked: boolean,
): AskUserAnswerDraft {
  if (question.type === 'single_select') {
    return {
      ...current,
      value: checked ? optionId : '',
    }
  }
  const selected = Array.isArray(current.value) ? current.value : []
  const next = checked
    ? [...new Set([...selected, optionId])]
    : selected.filter((id) => id !== optionId)
  return {
    ...current,
    value: next,
  }
}

export function AskUserCard({ widget, onSubmit, onCancel }: AskUserCardProps) {
  const [answers, setAnswers] = useState<Record<string, AskUserAnswerDraft>>(() =>
    buildInitialAnswers(widget.questions),
  )
  const [submitting, setSubmitting] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)

  const disabled = widget.status !== 'pending' || submitting
  const errors = useMemo(() => {
    return Object.fromEntries(
      widget.questions.map((question) => [
        question.id,
        validateQuestion(question, answers[question.id]),
      ]),
    )
  }, [answers, widget.questions])

  const hasErrors = Object.values(errors).some(Boolean)

  const updateAnswer = (questionId: string, updater: (prev: AskUserAnswerDraft) => AskUserAnswerDraft) => {
    setAnswers((prev) => ({
      ...prev,
      [questionId]: updater(
        prev[questionId] ?? {
          questionId,
          value: '',
          customValue: '',
        },
      ),
    }))
    if (notice) {
      setNotice(null)
    }
  }

  const handleSubmit = async () => {
    if (disabled || hasErrors) {
      return
    }
    setSubmitting(true)
    setNotice(null)
    try {
      await onSubmit?.({
        widgetId: widget.widgetId,
        kind: 'ask_user',
        answers: widget.questions.map((question) => answers[question.id]).filter(Boolean),
      })
      setNotice('已提交')
    } finally {
      setSubmitting(false)
    }
  }

  const handleCancel = async () => {
    if (disabled) {
      return
    }
    await onCancel?.({ widgetId: widget.widgetId, kind: 'ask_user' })
    setNotice('已取消')
  }

  return (
    <div className={`widget-card ask-user-card status-${widget.status}`} data-widget-id={widget.widgetId}>
      <header className="widget-card-header">
        <div>
          <div className="widget-card-eyebrow">Ask User</div>
          <div className="widget-card-title">{widget.title}</div>
        </div>
        <span className={`widget-card-status widget-card-status-${widget.status}`}>{widget.status}</span>
      </header>

      {widget.description ? <p className="widget-card-description">{widget.description}</p> : null}

      <div className="widget-question-list">
        {widget.questions.map((question) => {
          const draft = answers[question.id]
          const error = errors[question.id]
          return (
            <section key={question.id} className="widget-question-block">
              <label className="widget-question-label" htmlFor={`${widget.widgetId}-${question.id}`}>
                <span>{question.label}</span>
                {question.required ? <span className="widget-question-required">必填</span> : null}
              </label>
              {question.description ? (
                <p className="widget-question-description">{question.description}</p>
              ) : null}

              {question.type === 'text' || question.type === 'textarea' ? (
                question.type === 'textarea' ? (
                  <textarea
                    id={`${widget.widgetId}-${question.id}`}
                    aria-label={question.label}
                    className="widget-textarea"
                    rows={4}
                    disabled={disabled}
                    value={typeof draft?.value === 'string' ? draft.value : ''}
                    placeholder={question.placeholder}
                    maxLength={question.maxLength}
                    onChange={(event) =>
                      updateAnswer(question.id, (prev) => ({
                        ...prev,
                        value: event.target.value,
                      }))
                    }
                  />
                ) : (
                  <input
                    id={`${widget.widgetId}-${question.id}`}
                    aria-label={question.label}
                    className="widget-input"
                    disabled={disabled}
                    value={typeof draft?.value === 'string' ? draft.value : ''}
                    placeholder={question.placeholder}
                    maxLength={question.maxLength}
                    onChange={(event) =>
                      updateAnswer(question.id, (prev) => ({
                        ...prev,
                        value: event.target.value,
                      }))
                    }
                  />
                )
              ) : (
                <div className="widget-choice-list">
                  {question.options.map((option) => {
                    const selected = Array.isArray(draft?.value)
                      ? draft.value.includes(option.id)
                      : draft?.value === option.id
                    const isRecommended =
                      question.recommendedOptionId &&
                      question.recommendedOptionId === option.id
                    return (
                      <label key={option.id} className="widget-choice-item">
                        <input
                          aria-label={option.label}
                          type={question.type === 'multi_select' ? 'checkbox' : 'radio'}
                          name={question.id}
                          checked={selected}
                          disabled={disabled}
                          onChange={(event) =>
                            updateAnswer(question.id, (prev) =>
                              updateChoiceSelection(question, prev, option.id, event.target.checked),
                            )
                          }
                        />
                        <span>
                          <strong>
                            {option.label}
                            {isRecommended ? <em className="widget-choice-recommended">推荐</em> : null}
                          </strong>
                          {option.description ? <small>{option.description}</small> : null}
                        </span>
                      </label>
                    )
                  })}

                  {question.allowCustomInput ? (
                    <input
                      aria-label={`${question.label}-其他`}
                      className="widget-input"
                      disabled={disabled}
                      value={draft?.customValue ?? ''}
                      placeholder={question.customInputPlaceholder ?? '补充说明'}
                      onChange={(event) =>
                        updateAnswer(question.id, (prev) => ({
                          ...prev,
                          customValue: event.target.value,
                        }))
                      }
                    />
                  ) : null}
                </div>
              )}

              {error ? <div className="widget-question-error">{error}</div> : null}
            </section>
          )
        })}
      </div>

      {notice ? <div className="widget-card-notice">{notice}</div> : null}

      <footer className="widget-card-actions">
        {widget.allowSkip ? (
          <button type="button" className="widget-btn-secondary" disabled={disabled} onClick={() => void handleCancel()}>
            {widget.cancelLabel ?? '跳过'}
          </button>
        ) : null}
        <button type="button" className="widget-btn-primary" disabled={disabled || hasErrors} onClick={() => void handleSubmit()}>
          {submitting ? '提交中…' : widget.submitLabel ?? '提交'}
        </button>
      </footer>
    </div>
  )
}
