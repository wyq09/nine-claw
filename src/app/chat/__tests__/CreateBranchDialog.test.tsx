import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { CreateBranchDialog } from '../CreateBranchDialog'

function renderDialog(overrides: Partial<Parameters<typeof CreateBranchDialog>[0]> = {}) {
  const onConfirm = vi.fn()
  const onCancel = vi.fn()
  render(
    <CreateBranchDialog
      forkPointPrompt="帮我评估这个方案的风险"
      busy={false}
      onCancel={onCancel}
      onConfirm={onConfirm}
      {...overrides}
    />,
  )
  return { onConfirm, onCancel }
}

describe('CreateBranchDialog', () => {
  it('renders both workspace modes with share preselected as recommended', () => {
    renderDialog()
    expect(screen.getByText('创建对话分支')).toBeTruthy()
    expect(screen.getByText('共享工作空间')).toBeTruthy()
    expect(screen.getByText('独立拷贝')).toBeTruthy()
    expect(screen.getByText('推荐')).toBeTruthy()
    expect(screen.getByRole('radio', { name: /共享工作空间/ })).toHaveAttribute(
      'aria-checked',
      'true',
    )
    expect(screen.getByRole('radio', { name: /独立拷贝/ })).toHaveAttribute(
      'aria-checked',
      'false',
    )
  })

  it('confirms with the default share mode and shows a plain create label', () => {
    const { onConfirm } = renderDialog()
    const confirm = screen.getByRole('button', { name: '创建分支' })
    fireEvent.click(confirm)
    expect(onConfirm).toHaveBeenCalledWith('share')
  })

  it('switches to copy mode and relabels the confirm button', () => {
    const { onConfirm } = renderDialog()
    fireEvent.click(screen.getByRole('radio', { name: /独立拷贝/ }))
    expect(screen.getByRole('button', { name: '拷贝并创建' })).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: '拷贝并创建' }))
    expect(onConfirm).toHaveBeenCalledWith('copy')
  })

  it('disables interactions while busy', () => {
    renderDialog({ busy: true })
    const confirm = screen.getByRole('button', { name: '创建中…' })
    expect(confirm).toBeDisabled()
    expect(screen.getByRole('button', { name: '取消' })).toBeDisabled()
    expect(screen.getByRole('radio', { name: /独立拷贝/ })).toBeDisabled()
  })

  it('cancels via the cancel button', () => {
    const { onCancel } = renderDialog()
    fireEvent.click(screen.getByRole('button', { name: '取消' }))
    expect(onCancel).toHaveBeenCalledTimes(1)
  })

  it('cancels via Escape when idle but not while busy', () => {
    const { onCancel } = renderDialog()
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(onCancel).toHaveBeenCalledTimes(1)

    const busy = renderDialog({ busy: true })
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(busy.onCancel).not.toHaveBeenCalled()
  })

  it('shows the fork point prompt preview', () => {
    renderDialog()
    expect(screen.getByText(/帮我评估这个方案的风险/)).toBeTruthy()
  })
})
