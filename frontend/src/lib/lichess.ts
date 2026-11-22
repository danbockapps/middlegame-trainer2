// Client-side Lichess helpers reused from the predecessor project: 429-retrying fetch,
// cloud-eval prefetch while the user studies a position, and the screenshot URL.
// Cloud-eval and chess-api.com are CORS-open; the explorer and game export stay
// server-side (the backend queries them with its token).

import {Chess} from 'chess.js'

async function fetchWithRetry(url: string, init?: RequestInit, retries = 1): Promise<Response> {
  const res = await fetch(url, init)
  if (res.status === 429 && retries > 0) {
    const retryAfter = Number(res.headers.get('retry-after')) || 2
    await new Promise((r) => setTimeout(r, retryAfter * 1000))
    return fetchWithRetry(url, init, retries - 1)
  }
  return res
}

interface CloudEvalResponse {
  depth: number
  pvs: {moves: string; cp?: number; mate?: number}[]
}

/** Lichess cloud-eval for one position (White POV, pawns); null on a miss. */
export async function fetchCloudEval(
  fen: string,
): Promise<{cp: number | null; mate: number | null} | null> {
  const res = await fetchWithRetry(
    `https://lichess.org/api/cloud-eval?${new URLSearchParams({fen, multiPv: '6'})}`,
  )
  if (!res.ok) return null
  const data = (await res.json()) as CloudEvalResponse
  const pv = data.pvs?.[0]
  if (!pv) return null
  return {cp: pv.cp ?? null, mate: pv.mate ?? null}
}

// chess-api.com fallback when cloud-eval misses (different engine, so only used when
// cloud-eval has nothing for the position).
interface ChessApiResponse {
  eval?: number
  centipawns?: number
  mate?: number | null
}

export async function fetchChessApiEval(
  fen: string,
): Promise<{cp: number | null; mate: number | null}> {
  const res = await fetch('https://chess-api.com/v1', {
    method: 'POST',
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify({fen, depth: 12}),
  })
  if (!res.ok) throw new Error(`chess-api.com error (${res.status})`)
  const data = (await res.json()) as ChessApiResponse
  return {
    cp: typeof data.centipawns === 'number' ? data.centipawns : (data.eval ?? 0) * 100,
    mate: data.mate ?? null,
  }
}

/**
 * Eval of one position, White POV, in pawns (or null on total failure). Cloud-eval
 * first, chess-api.com fallback. Used to prefetch the shown position's eval while the
 * user is still thinking; the after-move eval comes from the backend result.
 */
export async function fetchPositionEval(fen: string): Promise<number | null> {
  try {
    const cloud = await fetchCloudEval(fen)
    if (cloud) return cloud.mate !== null ? null : cloud.cp !== null ? cloud.cp / 100 : null
  } catch {
    // fall through to chess-api
  }
  try {
    const fallback = await fetchChessApiEval(fen)
    return fallback.cp !== null ? fallback.cp / 100 : null
  } catch {
    return null
  }
}

/** Lichess board-screenshot URL for a position plus the move just played. */
export function screenshotUrl(
  fen: string,
  color: 'white' | 'black',
  lastMove?: {from: string; to: string} | null,
): string {
  const params = new URLSearchParams({fen, color})
  if (lastMove) params.set('lastMove', lastMove.from + lastMove.to)
  return `https://lichess.org/export/fen.gif?${params.toString()}`
}

/** Normalize a Lichess-style king-takes-rook castle UCI (e1h1) to king-two-squares (e1g1). */
export function normalizeCastleUci(fen: string, uci: string): string {
  const chess = new Chess(fen)
  try {
    chess.move({from: uci.slice(0, 2), to: uci.slice(2, 4)})
  } catch {
    return uci
  }
  const history = chess.history({verbose: true})
  const last = history[history.length - 1]
  return last ? last.from + last.to : uci
}
