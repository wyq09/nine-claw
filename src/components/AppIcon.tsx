export type IconName =
  | 'arrow-down'
  | 'arrow-left'
  | 'arrow-up'
  | 'attachment'
  | 'bag'
  | 'book'
  | 'bot'
  | 'broadcast'
  | 'chevron-down'
  | 'clock'
  | 'close'
  | 'download'
  | 'folder'
  | 'keyboard'
  | 'message'
  | 'more'
  | 'network'
  | 'panel'
  | 'plus'
  | 'plus-circle'
  | 'provider'
  | 'puzzle'
  | 'refresh'
  | 'search'
  | 'send'
  | 'settings'
  | 'spark'
  | 'sparkles'
  | 'stop'
  | 'trash'
  | 'wrench'
  | 'zap'
  | 'upload'
  | 'qr'
  | 'check'
  | 'eye'
  | 'eye-off'
  | 'users'

export function AppIcon({ name, size = 20 }: { name: IconName; size?: number }) {
  const stroke = 1.8

  return (
    <svg
      aria-hidden="true"
      className="app-icon"
      fill="none"
      height={size}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      {name === 'arrow-left' ? (
        <path d="M15 6 9 12l6 6M9 12h10" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
      {name === 'arrow-down' ? (
        <path d="M12 5v14M6 13l6 6 6-6" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
      {name === 'arrow-up' ? (
        <path
          d="M12 19V5M5 12l7-7 7 7"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={stroke}
        />
      ) : null}
      {name === 'plus' ? <path d="M12 5v14M5 12h14" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} /> : null}
      {name === 'spark' ? (
        <path d="m12 3 1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8L12 3Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
      {name === 'zap' ? (
        <path d="M13.5 3 6.8 12h4.4L10.5 21l6.7-9h-4.4L13.5 3Z" fill="currentColor" />
      ) : null}
      {name === 'book' ? (
        <>
          <path d="M5 5.5C5 4.67 5.67 4 6.5 4H19v15H6.5A1.5 1.5 0 0 1 5 17.5v-12Z" stroke="currentColor" strokeWidth={stroke} />
          <path d="M9 4v15" stroke="currentColor" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'clock' ? (
        <>
          <circle cx="12" cy="12" r="8" stroke="currentColor" strokeWidth={stroke} />
          <path d="M12 8v4l3 2" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'network' ? (
        <>
          <circle cx="6" cy="12" r="2.2" stroke="currentColor" strokeWidth={stroke} />
          <circle cx="18" cy="7" r="2.2" stroke="currentColor" strokeWidth={stroke} />
          <circle cx="18" cy="17" r="2.2" stroke="currentColor" strokeWidth={stroke} />
          <path d="M8 11l7.6-3M8 13l7.6 3" stroke="currentColor" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'settings' ? (
        <>
          <circle cx="12" cy="12" r="3" stroke="currentColor" strokeWidth={stroke} />
          <path
            d="M19.4 15a1 1 0 0 0 .2 1.1l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1 1 0 0 0-1.1-.2 1 1 0 0 0-.6.9V20a2 2 0 1 1-4 0v-.2a1 1 0 0 0-.6-.9 1 1 0 0 0-1.1.2l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1 1 0 0 0 .2-1.1 1 1 0 0 0-.9-.6H4a2 2 0 1 1 0-4h.2a1 1 0 0 0 .9-.6 1 1 0 0 0-.2-1.1l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1 1 0 0 0 1.1.2h.1a1 1 0 0 0 .6-.9V4a2 2 0 1 1 4 0v.2a1 1 0 0 0 .6.9 1 1 0 0 0 1.1-.2l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1 1 0 0 0-.2 1.1v.1a1 1 0 0 0 .9.6H20a2 2 0 1 1 0 4h-.2a1 1 0 0 0-.9.6Z"
            stroke="currentColor"
            strokeLinejoin="round"
            strokeWidth={1.4}
          />
        </>
      ) : null}
      {name === 'panel' ? (
        <>
          <rect x="4" y="4" width="16" height="16" rx="2" stroke="currentColor" strokeWidth={stroke} />
          <path d="M11 4v16M15 10l-2 2 2 2" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'more' ? (
        <>
          <circle cx="6" cy="12" r="1.5" fill="currentColor" />
          <circle cx="12" cy="12" r="1.5" fill="currentColor" />
          <circle cx="18" cy="12" r="1.5" fill="currentColor" />
        </>
      ) : null}
      {name === 'chevron-down' ? <path d="m6 9 6 6 6-6" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} /> : null}
      {name === 'bot' ? (
        <>
          <rect x="6" y="8" width="12" height="10" rx="3" stroke="currentColor" strokeWidth={stroke} />
          <path d="M12 4v4M9.5 13h.01M14.5 13h.01M8 18v2M16 18v2" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'attachment' ? <path d="M8.5 12.5 14 7a3 3 0 1 1 4.2 4.2l-6.8 6.8a5 5 0 1 1-7.1-7.1l7.1-7.1" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} /> : null}
      {name === 'send' ? <path d="m5 12 14-7-3 14-4.2-5.2L5 12Z" fill="currentColor" /> : null}
      {name === 'stop' ? (
        <rect
          x="4.8"
          y="4.8"
          width="14.4"
          height="14.4"
          rx="2.88"
          ry="2.88"
          fill="currentColor"
        />
      ) : null}
      {name === 'search' ? (
        <>
          <circle cx="11" cy="11" r="5.5" stroke="currentColor" strokeWidth={stroke} />
          <path d="m16 16 3.5 3.5" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'refresh' ? (
        <>
          <path
            d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"
            stroke="currentColor"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={stroke}
          />
          <path d="M21 3v5h-5" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
          <path
            d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"
            stroke="currentColor"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={stroke}
          />
          <path d="M3 21v-5h5" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'bag' ? (
        <>
          <path d="M6 8h12l-1 11H7L6 8Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={stroke} />
          <path d="M9 8a3 3 0 1 1 6 0" stroke="currentColor" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'plus-circle' ? (
        <>
          <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth={stroke} />
          <path d="M12 8v8M8 12h8" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'upload' ? (
        <>
          <path d="M12 16V6M8 10l4-4 4 4" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
          <path d="M5 18h14" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'download' ? (
        <>
          <path d="M12 8v10M8 14l4 4 4-4" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
          <path d="M5 20h14" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'folder' ? (
        <>
          <path d="M4 8h5l2 2h9v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={stroke} />
          <path d="M4 10h16" stroke="currentColor" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'puzzle' ? (
        <path d="M10 4h4v3a1.5 1.5 0 1 0 3 0V4h3v4a2 2 0 0 1-2 2h-3v3a1.5 1.5 0 1 1-3 0v-3H8a2 2 0 0 1-2-2V4h3a1.5 1.5 0 1 0 1 0Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
      {name === 'trash' ? (
        <>
          <path d="M5 7h14M9 7V5h6v2M8 10v7M12 10v7M16 10v7M7 7l1 12h8l1-12" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'close' ? <path d="m6 6 12 12M18 6 6 18" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} /> : null}
      {name === 'sparkles' ? (
        <>
          <path d="M6 4 7.4 7.6 11 9l-3.6 1.4L6 14l-1.4-3.6L1 9l3.6-1.4L6 4ZM18 9l1.1 2.9L22 13l-2.9 1.1L18 17l-1.1-2.9L14 13l2.9-1.1L18 9ZM16 2l.7 1.8L18.5 4.5l-1.8.7L16 7l-.7-1.8-1.8-.7 1.8-.7L16 2Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={1.4} />
        </>
      ) : null}
      {name === 'message' ? (
        <path d="M5 6.5A2.5 2.5 0 0 1 7.5 4H18a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H11l-4.5 4v-4H7.5A2.5 2.5 0 0 1 5 12.5v-6Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
      {name === 'keyboard' ? (
        <>
          <rect x="3" y="6" width="18" height="12" rx="2.5" stroke="currentColor" strokeWidth={stroke} />
          <path d="M6.5 10h.01M10 10h.01M13.5 10h.01M17 10h.01M7 14h10" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'provider' ? (
        <>
          <rect x="4" y="5" width="16" height="5" rx="1.5" stroke="currentColor" strokeWidth={stroke} />
          <rect x="4" y="14" width="16" height="5" rx="1.5" stroke="currentColor" strokeWidth={stroke} />
          <path d="M8 10v4M16 10v4" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'broadcast' ? (
        <>
          <path d="M12 18a6 6 0 0 0 0-12M12 14a2 2 0 0 0 0-4M5 12a9 9 0 0 1 3-6.7M19 12a9 9 0 0 0-3-6.7" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
          <circle cx="12" cy="12" r="1.6" fill="currentColor" />
        </>
      ) : null}
      {name === 'wrench' ? (
        <path
          d="M14.5 6.5a4 4 0 0 0 2.8 5.6l-6.9 6.9a2 2 0 1 1-2.8-2.8l6.9-6.9a4 4 0 0 1-5.6-2.8l2.4-2.4 2.6.6.6 2.6 2.4 2.4-.4.4"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={stroke}
        />
      ) : null}
      {name === 'qr' ? (
        <>
          <rect x="5" y="5" width="5" height="5" rx="1" stroke="currentColor" strokeWidth={stroke} />
          <rect x="14" y="5" width="5" height="5" rx="1" stroke="currentColor" strokeWidth={stroke} />
          <rect x="5" y="14" width="5" height="5" rx="1" stroke="currentColor" strokeWidth={stroke} />
          <rect x="14" y="14" width="3" height="3" rx="0.5" stroke="currentColor" strokeWidth={stroke} />
          <path d="M17 14v-1M17 19h-1" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'check' ? (
        <path d="M5 13 9 17 19 7" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
      {name === 'eye' ? (
        <>
          <path
            d="M2.5 12s3.4-6 9.5-6 9.5 6 9.5 6-3.4 6-9.5 6-9.5-6-9.5-6Z"
            stroke="currentColor"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={stroke}
          />
          <circle cx="12" cy="12" r="3" stroke="currentColor" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'eye-off' ? (
        <>
          <path
            d="M3 3l18 18M10.7 6.2A10.6 10.6 0 0 1 12 6c6.1 0 9.5 6 9.5 6a16.4 16.4 0 0 1-4.1 4.6M6.2 6.7A15.3 15.3 0 0 0 2.5 12s3.4 6 9.5 6c1.5 0 2.8-.3 4-.8M10.2 10.2A3 3 0 0 0 12 15a3 3 0 0 0 1.8-.6"
            stroke="currentColor"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={stroke}
          />
        </>
      ) : null}
      {name === 'users' ? (
        <>
          <circle cx="9" cy="9" r="3.2" stroke="currentColor" strokeWidth={stroke} />
          <circle cx="17" cy="10" r="2.5" stroke="currentColor" strokeWidth={stroke} />
          <path
            d="M3 19c0-2.8 2.7-5 6-5s6 2.2 6 5M15 19c0-1.8 1.4-3.5 3.5-3.5S22 17.2 22 19"
            stroke="currentColor"
            strokeLinecap="round"
            strokeWidth={stroke}
          />
        </>
      ) : null}
    </svg>
  )
}
