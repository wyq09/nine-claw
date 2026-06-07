export type HistoryContextMenuState =
  | {
      kind: 'session'
      sessionId: string
      title: string
      x: number
      y: number
      canDelete: boolean
      pinned: boolean
      groupId: string | null
    }
  | {
      kind: 'group'
      groupId: string
      title: string
      x: number
      y: number
    }
