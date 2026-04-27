import { useMemo, useState } from 'react'
import { AppIcon } from './AppIcon'
import { normalizeLocalAssetSource } from '../lib/inlineMedia'

export type AgentAvatarProps = {
  name: string
  avatarUri?: string | null
  accentColor?: string | null
  className?: string
  imageClassName?: string
  initialClassName?: string
  size?: number
  fallbackToIcon?: boolean
}

function pickInitial(name: string): string {
  const first = Array.from(name.trim())[0]
  return first ? first.toUpperCase() : '?'
}

export function AgentAvatar({
  name,
  avatarUri,
  accentColor,
  className = '',
  imageClassName = '',
  initialClassName = '',
  size = 20,
  fallbackToIcon = false,
}: AgentAvatarProps) {
  const [imageFailed, setImageFailed] = useState(false)
  const normalizedSrc = useMemo(() => {
    const trimmed = avatarUri?.trim() ?? ''
    return trimmed ? normalizeLocalAssetSource(trimmed) : ''
  }, [avatarUri])

  const style = accentColor
    ? {
        borderColor: `${accentColor}55`,
        background: `${accentColor}16`,
        color: accentColor,
      }
    : undefined

  const showImage = Boolean(normalizedSrc) && !imageFailed

  return (
    <span className={`agent-avatar ${className}`.trim()} style={style} aria-hidden>
      {showImage ? (
        <img
          className={`agent-avatar-image ${imageClassName}`.trim()}
          src={normalizedSrc}
          alt=""
          onError={() => setImageFailed(true)}
        />
      ) : fallbackToIcon ? (
        <AppIcon name="bot" size={size} />
      ) : (
        <span className={`agent-avatar-initial ${initialClassName}`.trim()}>{pickInitial(name)}</span>
      )}
    </span>
  )
}

export default AgentAvatar
