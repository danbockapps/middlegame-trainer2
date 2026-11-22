// Chessground wrapper. Rebuilds dests/lastMove/orientation via api.set on prop changes.
import {Chess} from 'chess.js'
import {useEffect, useRef} from 'react'
import {Chessground} from 'chessground'
import type {Api} from 'chessground/api'
import type {Key} from 'chessground/types'
import 'chessground/assets/chessground.base.css'
import 'chessground/assets/chessground.brown.css'
import 'chessground/assets/chessground.cburnett.css'

export interface BoardProps {
  fen: string
  orientation: 'white' | 'black'
  /** Which color(s) may move. `null` disables interaction entirely. */
  movableColor: 'white' | 'black' | 'both' | null
  /** Legal destinations for the movable color; null means free movement (editor browsing). */
  dests: Map<string, string[]> | null
  onMove?: (from: string, to: string) => void
  lastMove?: [string, string] | null
  /** Bump to force a re-sync from `fen` (undoes chessground's optimistic piece move). */
  resetSignal?: number
}

export function Board(props: BoardProps) {
  const ref = useRef<HTMLDivElement | null>(null)
  const apiRef = useRef<Api | null>(null)
  const onMoveRef = useRef(props.onMove)
  useEffect(() => {
    onMoveRef.current = props.onMove
  })

  useEffect(() => {
    if (!ref.current) return
    const api = Chessground(ref.current, {
      animation: {duration: 180},
      movable: {free: false, showDests: true},
      drawable: {enabled: false},
    })
    apiRef.current = api
    return () => {
      api.destroy()
      apiRef.current = null
    }
  }, [])

  useEffect(() => {
    const api = apiRef.current
    if (!api) return
    const canMove = props.movableColor !== null
    api.set({
      fen: props.fen,
      orientation: props.orientation,
      turnColor: turnColorOf(props.fen), // chessground does NOT infer the turn from the fen
      check: inCheck(props.fen),
      lastMove: props.lastMove ? (props.lastMove.map(key) as [Key, Key]) : undefined,
      viewOnly: !canMove,
      movable: {
        free: props.dests === null && canMove,
        color: props.movableColor === 'both' ? 'both' : (props.movableColor as 'white' | 'black'),
        showDests: true,
        dests: props.dests ? asDests(props.dests) : undefined,
        events: {
          after: (from: Key, to: Key) => onMoveRef.current?.(from, to),
        },
      },
    })
  }, [
    props.fen,
    props.orientation,
    props.movableColor,
    props.dests,
    props.lastMove,
    props.resetSignal,
  ])

  return <div className="board" ref={ref} />
}

function inCheck(fen: string): boolean {
  try {
    return new Chess(fen).inCheck()
  } catch {
    return false
  }
}

// Chessground treats whose turn it is as a separate movable/turnColor concern, not
// something it parses out of the fen — pass it explicitly or black positions only
// accept premoves (piece never moves).
function turnColorOf(fen: string): 'white' | 'black' {
  return fen.split(' ')[1] === 'b' ? 'black' : 'white'
}

// Chessground's `Key` is a template-literal type; our square strings satisfy it
// wherever they come from chess.js. Cast at this boundary.
const key = (s: string): Key => s as Key

function asDests(dests: Map<string, string[]>): Map<Key, Key[]> {
  const out = new Map<Key, Key[]>()
  for (const [from, tos] of dests) out.set(key(from), tos.map(key))
  return out
}
