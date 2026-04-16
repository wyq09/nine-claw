import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { getSessionContextStats, type SessionContextStatsPayload } from '../lib/piClient'
import type { ConversationTurn, ProviderConfig } from '../types'
import {
  createSessionContextState,
  mergeSessionContextTokensFromTurns,
  resolveContextWindow,
  type ContextWindowSource,
  type SessionContextState,
} from '../app/lib/sessionContext'

const EMPTY_TURNS: ConversationTurn[] = []

type UseSessionContextWindowInput = {
  sessionId: string
  providerConfig: Pick<ProviderConfig, 'maxContextTokens'> | null
  /** 用于在 Rust/PI 占位统计为 0 时从本会话 turns 累计真实 token */
  turns?: ConversationTurn[] | undefined
}

type UseSessionContextWindowResult = {
  state: SessionContextState | null
  loading: boolean
  error: string | null
  refresh: () => void
}

export function useSessionContextWindow(
  input: UseSessionContextWindowInput,
): UseSessionContextWindowResult {
  const { sessionId, providerConfig, turns: turnsProp } = input
  const turns = turnsProp ?? EMPTY_TURNS
  const [rpcStats, setRpcStats] = useState<SessionContextStatsPayload | null>(null)
  const [loading, setLoading] = useState(() => Boolean(sessionId.trim()))
  const [error, setError] = useState<string | null>(null)
  const abortRef = useRef<AbortController | null>(null)
  const mountedRef = useRef(true)

  const refresh = useCallback(() => {
    abortRef.current?.abort()
    const controller = new AbortController()
    abortRef.current = controller

    if (!sessionId.trim()) {
      setRpcStats(null)
      setLoading(false)
      setError(null)
      return
    }

    setRpcStats(null)
    setLoading(true)
    setError(null)

    void (async () => {
      try {
        const stats = await getSessionContextStats(sessionId)
        if (!mountedRef.current || controller.signal.aborted) return
        setRpcStats(stats)
      } catch (err) {
        if (!mountedRef.current || controller.signal.aborted) return
        setRpcStats(null)
        setError(err instanceof Error ? err.message : String(err))
      } finally {
        if (mountedRef.current && !controller.signal.aborted) {
          setLoading(false)
        }
      }
    })()
  }, [sessionId])

  useEffect(() => {
    refresh()
    return () => {
      abortRef.current?.abort()
    }
  }, [refresh])

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
    }
  }, [])

  const state = useMemo((): SessionContextState | null => {
    if (!sessionId.trim() || !rpcStats) return null
    const merged = mergeSessionContextTokensFromTurns(
      {
        usedTokens: rpcStats.usedTokens,
        inputTokens: rpcStats.inputTokens,
        outputTokens: rpcStats.outputTokens,
      },
      turns,
    )
    const resolved = resolveContextWindow({
      autoDetected: rpcStats.contextWindow ?? undefined,
      manualConfig: providerConfig?.maxContextTokens,
    })
    return createSessionContextState({
      sessionId,
      usedTokens: merged.usedTokens,
      contextWindow: resolved.tokens,
      model: rpcStats.model ?? undefined,
      source: resolved.source as ContextWindowSource,
      inputTokens: merged.inputTokens,
      outputTokens: merged.outputTokens,
    })
  }, [sessionId, rpcStats, turns, providerConfig?.maxContextTokens])

  return { state, loading, error, refresh }
}
