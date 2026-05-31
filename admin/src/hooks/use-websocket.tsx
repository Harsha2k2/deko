import { createContext, useContext, useEffect, useRef, useState, useCallback, type ReactNode } from 'react'
import { toast } from 'sonner'

export interface WsVerdictEvent {
  type: 'verdict'
  action_id: string
  agent_id: string
  decision: string
  reason: string
  risk_level: string
  provider: string
}

export type WsEvent = WsVerdictEvent

interface WsState {
  connected: boolean
  lastEvent: WsEvent | null
}

const WsContext = createContext<WsState>({ connected: false, lastEvent: null })

let globalListeners: Array<(event: WsEvent) => void> = []

export function addWsListener(fn: (event: WsEvent) => void) {
  globalListeners.push(fn)
  return () => {
    globalListeners = globalListeners.filter(l => l !== fn)
  }
}

export function WsProvider({ children }: { children: ReactNode }) {
  const [connected, setConnected] = useState(false)
  const [lastEvent, setLastEvent] = useState<WsEvent | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const connect = useCallback(() => {
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const url = `${proto}//${window.location.host}/api/admin/ws`
    const ws = new WebSocket(url)

    ws.onopen = () => setConnected(true)

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as WsEvent
        setLastEvent(data)

        if (data.type === 'verdict' && (data.decision === 'denied' || data.decision === 'escalate')) {
          toast(
            `Action ${data.decision === 'denied' ? 'Denied' : 'Escalated'}`,
            {
              description: data.reason.slice(0, 120),
              duration: 5000,
            }
          )
        }

        globalListeners.forEach(fn => fn(data))
      } catch {
        // ignore non-JSON messages (like ping)
      }
    }

    ws.onclose = () => {
      setConnected(false)
      reconnectTimerRef.current = setTimeout(connect, 3000)
    }

    ws.onerror = () => {
      ws.close()
    }

    wsRef.current = ws
  }, [])

  useEffect(() => {
    connect()
    return () => {
      clearTimeout(reconnectTimerRef.current)
      wsRef.current?.close()
    }
  }, [connect])

  return (
    <WsContext.Provider value={{ connected, lastEvent }}>
      {children}
    </WsContext.Provider>
  )
}

export function useWs() {
  return useContext(WsContext)
}
