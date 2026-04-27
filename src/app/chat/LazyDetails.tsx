import { useRef, useState, type DetailsHTMLAttributes, type ReactNode } from 'react'

type LazyDetailsProps = Omit<DetailsHTMLAttributes<HTMLDetailsElement>, 'children'> & {
  summary: ReactNode
  summaryClassName?: string
  children: ReactNode | ((open: boolean) => ReactNode)
  defaultOpen?: boolean
}

export function LazyDetails({
  summary,
  summaryClassName,
  children,
  defaultOpen = false,
  onToggle,
  ...rest
}: LazyDetailsProps) {
  const [open, setOpen] = useState(Boolean(defaultOpen))
  const renderedRef = useRef(Boolean(defaultOpen))
  if (open) {
    renderedRef.current = true
  }

  return (
    <details
      {...rest}
      open={open}
      onToggle={(event) => {
        const nextOpen = event.currentTarget.open
        if (nextOpen) {
          renderedRef.current = true
        }
        setOpen(nextOpen)
        onToggle?.(event)
      }}
    >
      <summary className={summaryClassName}>{summary}</summary>
      {renderedRef.current ? (typeof children === 'function' ? children(open) : children) : null}
    </details>
  )
}
