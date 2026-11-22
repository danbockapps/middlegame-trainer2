// Types mirroring docs/api-contract.md exactly.

export type Side = 'white' | 'black'
export type TimeControl = 'blitz' | 'rapid' | 'classical'

export interface User {
  id: string
  username: string
  ratings: {blitz: number | null; rapid: number | null; classical: number | null}
}

export interface OpeningSummary {
  id: number
  name: string | null
  side: Side
  nameOverridden: boolean
  rangeMin: number
  rangeMax: number
  moveCount: number
}

export interface Opening extends OpeningSummary {
  moves: string[] // UCI, DFS pre-order
  deleted: boolean
}

export interface QuizPosition {
  fen: string
  moveNumber: number
  side: Side
  gameId: string
  gameUrl: string
}

export interface QuizStart {
  quizId: string
  position: QuizPosition
}

export interface MastersTopMove {
  san: string
  percent: number
}

export interface QuizMasters {
  playedPercent: number
  positionTotal: number
  topMoves: MastersTopMove[]
}

/**
 * Evals are pawns, White POV. When an eval is a mate score the contract encodes it as
 * `"mate": n` instead of the before/after numbers; we normalize at the fetch boundary
 * (see client.ts) into explicit nullable fields.
 */
export interface QuizEval {
  before: number | null
  after: number | null
  /** When set, the after-position eval is a mate score (positive: mate delivered by White). */
  mate: number | null
  source: string
}

export interface QuizMoveResult {
  masters: QuizMasters
  eval: QuizEval
}

export interface SearchSettings {
  timeControl: TimeControl
  minRating: number
  maxRating: number
}

export interface ApiError {
  error: string
  message: string
}

/** Thrown by every client call on a non-2xx response. */
export class RequestError extends Error {
  readonly status: number
  readonly code: string

  constructor(status: number, body: ApiError | null) {
    super(body?.message ?? `Request failed (${status})`)
    this.status = status
    this.code = body?.error ?? 'unknown'
    this.name = 'RequestError'
  }
}
