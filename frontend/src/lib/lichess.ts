// Client-side Lichess helpers reused from the predecessor project: 429-retrying fetch,
// cloud-eval prefetch while the user studies a position, and the screenshot URL.
// Cloud-eval and stockfish.online are CORS-open; the explorer and game export stay
// server-side (the backend queries them with its token).

import {Chess} from 'chess.js'

async function fetchWithRetry(url: string, init?: RequestInit, retries = 1): Promise<Response> {
  console.debug(`[external api] request: GET ${url}`)
  const start = performance.now()
  const res = await fetch(url, init)
  console.debug(
    `[external api] response: GET ${url} -> ${res.status} in ${Math.round(performance.now() - start)}ms`,
  )
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

// stockfish.online fallback when cloud-eval misses (different engine, so only used when
// cloud-eval has nothing for the position). Depth is fixed at 15 per the API.
const STOCKFISH_KEY = import.meta.env.VITE_STOCKFISH_API_KEY as string | undefined

interface StockfishResponse {
  success: boolean
  error?: string
  evaluation?: number | null
  mate?: number | null
}

export async function fetchStockfishEval(
  fen: string,
): Promise<{cp: number | null; mate: number | null}> {
  const params = new URLSearchParams({fen, depth: '15'})
  if (STOCKFISH_KEY) params.set('key', STOCKFISH_KEY)
  const url = `https://stockfish.online/api/s/v2.php?${params.toString()}`
  console.debug(`[external api] request: GET ${url.replace(/&key=[^&]+/, '&key=…')}`)
  const start = performance.now()
  const res = await fetch(url)
  console.debug(
    `[external api] response: GET https://stockfish.online/api/s/v2.php -> ${res.status} in ${Math.round(performance.now() - start)}ms`,
  )
  if (!res.ok) throw new Error(`stockfish.online error (${res.status})`)
  const data = (await res.json()) as StockfishResponse
  if (!data.success) throw new Error(`stockfish.online error: ${data.error ?? 'unknown'}`)
  return {cp: data.evaluation ?? null, mate: data.mate ?? null}
}

/**
 * Eval of one position, White POV, in pawns (or null on total failure). Cloud-eval
 * first, stockfish.online fallback. Used to prefetch the shown position's eval while the
 * user is still thinking; the after-move eval comes from the backend result.
 */
export async function fetchPositionEval(fen: string): Promise<number | null> {
  try {
    const cloud = await fetchCloudEval(fen)
    if (cloud) return cloud.mate !== null ? null : cloud.cp !== null ? cloud.cp / 100 : null
  } catch {
    // fall through to stockfish.online
  }
  try {
    const fallback = await fetchStockfishEval(fen)
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
